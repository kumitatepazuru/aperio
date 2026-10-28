use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Data, DeriveInput, Fields, ItemStruct};

mod plugin_events;
mod register_submodules;

struct GeneratePartialInput {
    source: ItemStruct,
    partial: ItemStruct,
}

impl Parse for GeneratePartialInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let source: ItemStruct = input.parse()?;
        let partial: ItemStruct = input.parse()?;
        Ok(Self { source, partial })
    }
}

/// source struct と partial struct を並べて書くと、partial に書いていないフィールドを
/// source から自動補完して `apply_to` メソッドも生成するマクロ。
///
/// ```rust,ignore
/// generate_partial! {
///     #[napi(object)]
///     pub struct SyncableState {
///         pub viewer_state: ViewerState,
///         #[napi(ts_type = "string | null")]
///         pub main_selected_item_id: NullOr<String>,
///         // ...
///     }
///
///     /// 手動指定が必要なフィールドだけ書く。残りは source から自動生成される。
///     #[napi(object)]
///     pub struct SyncableStatePartial {
///         #[napi(ts_type = "string | null | undefined")]
///         pub main_selected_item_id: Option<NullOr<String>>,
///     }
/// }
/// ```
///
/// - partial に書いていないフィールドは `Option<T>` で自動追加される
/// - フィールドの順序は source に準拠する
#[proc_macro]
pub fn generate_partial(input: TokenStream) -> TokenStream {
    let GeneratePartialInput { source, partial } =
        parse_macro_input!(input as GeneratePartialInput);

    let source_name = &source.ident;
    let partial_name = &partial.ident;

    let source_fields = match &source.fields {
        Fields::Named(f) => &f.named,
        _ => panic!("generate_partial: source struct must have named fields"),
    };

    // partial に手動定義されているフィールドをマップ化
    let partial_overrides: HashMap<String, &syn::Field> = match &partial.fields {
        Fields::Named(f) => f
            .named
            .iter()
            .filter_map(|field| field.ident.as_ref().map(|i| (i.to_string(), field)))
            .collect(),
        _ => HashMap::new(),
    };

    let mut all_partial_fields: Vec<TokenStream2> = vec![];
    let mut apply_stmts: Vec<TokenStream2> = vec![];

    // source のフィールド順に処理
    for field in source_fields {
        let name = field.ident.as_ref().unwrap();
        let name_str = name.to_string();
        let ty = &field.ty;

        if let Some(override_field) = partial_overrides.get(&name_str) {
            // 手動定義をそのまま使用
            all_partial_fields.push(quote! { #override_field , });
        } else {
            // source のアトリビュートをそのまま引き継ぎ Option<T> で自動生成
            let field_attrs = &field.attrs;
            all_partial_fields.push(quote! {
                #(#field_attrs)*
                pub #name: Option<#ty>,
            });
        }

        apply_stmts.push(quote! {
            if let Some(v) = self.#name.take() { s.#name = v; }
        });
    }

    let partial_struct_attrs = &partial.attrs;
    let partial_vis = &partial.vis;

    quote! {
        #source

        #(#partial_struct_attrs)*
        #partial_vis struct #partial_name {
            #(#all_partial_fields)*
        }

        impl #partial_name {
            pub fn apply_to(&mut self, s: &mut #source_name) {
                #(#apply_stmts)*
            }
        }
    }
    .into()
}

#[proc_macro]
pub fn impl_plugin_event_methods(input: TokenStream) -> TokenStream {
    plugin_events::impl_plugin_event_methods(input)
}


/// `#[pymodule] mod` に適用すると、サブモジュールをすべて `sys.modules` に
/// 自動登録する `#[pymodule_init]` を生成する。
///
/// ```rust,ignore
/// #[register_submodules]
/// #[pymodule]
/// mod aperio {
///     #[pymodule_export]
///     use super::avloader::avloader_register;
/// }
/// ```
#[proc_macro_attribute]
pub fn register_submodules(attr: TokenStream, item: TokenStream) -> TokenStream {
    register_submodules::register_submodules(attr, item)
}

#[proc_macro_derive(PyDataclass)]
pub fn pydataclass(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("PyDataclass requires named fields"),
        },
        _ => panic!("PyDataclass only supports structs"),
    };

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let new_params = field_names
        .iter()
        .zip(field_types.iter())
        .map(|(n, t)| quote! { #n: #t });
    let new_body = field_names.iter().map(|n| quote! { #n });

    let repr_fmt = field_names
        .iter()
        .map(|n| format!("{}={{:?}}", n))
        .collect::<Vec<_>>()
        .join(", ");
    let repr_format_str = format!("{}({})", name, repr_fmt);
    let repr_args = field_names.iter().map(|n| quote! { self.#n });

    quote! {
        #[pyo3::pymethods]
        impl #name {
            #[new]
            fn new(#(#new_params),*) -> Self {
                Self { #(#new_body),* }
            }

            fn __repr__(&self) -> String {
                format!(#repr_format_str, #(#repr_args),*)
            }
        }
    }
    .into()
}
