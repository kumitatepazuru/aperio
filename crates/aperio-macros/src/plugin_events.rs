use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::Comma,
    Ident, Path, Result, Token, Type,
};

/// `名前: 型` の引数
struct EventArg {
    name: Ident,
    ty: Type,
}

impl Parse for EventArg {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        Ok(EventArg { name, ty })
    }
}

enum EventKind {
    /// `call fn_name(Event): Params -> Return`
    /// 指定プラグインのハンドラーを1つ呼ぶ(PluginManager.call_event)
    Call {
        params_type: Type,
        return_type: Type,
    },
    /// `reduce fn_name(Event): (arg: Type, ...) -> Value`
    /// 全プラグインのハンドラーを handler(*args, 現在の値) として畳み込む(PluginManager.reduce_event)
    Reduce {
        args: Punctuated<EventArg, Comma>,
        value_type: Type,
    },
}

struct EventEntry {
    fn_name: Ident,
    event_path: Path,
    kind: EventKind,
}

impl Parse for EventEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let kind_ident: Ident = input.parse()?;
        let fn_name: Ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        let event_path: Path = content.parse()?;
        input.parse::<Token![:]>()?;

        let kind = match kind_ident.to_string().as_str() {
            "call" => {
                let params_type: Type = input.parse()?;
                input.parse::<Token![->]>()?;
                let return_type: Type = input.parse()?;
                EventKind::Call {
                    params_type,
                    return_type,
                }
            }
            "reduce" => {
                let args_content;
                parenthesized!(args_content in input);
                let args = Punctuated::parse_terminated(&args_content)?;
                input.parse::<Token![->]>()?;
                let value_type: Type = input.parse()?;
                EventKind::Reduce { args, value_type }
            }
            _ => {
                return Err(syn::Error::new(
                    kind_ident.span(),
                    "expected `call` or `reduce`",
                ))
            }
        };

        Ok(EventEntry {
            fn_name,
            event_path,
            kind,
        })
    }
}

struct EventInput {
    struct_name: Ident,
    entries: Punctuated<EventEntry, Comma>,
}

impl Parse for EventInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let struct_name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        Ok(EventInput {
            struct_name,
            entries: Punctuated::parse_terminated(&content)?,
        })
    }
}

fn expand_entry(entry: &EventEntry) -> TokenStream2 {
    let fn_name = &entry.fn_name;
    let event_path = &entry.event_path;
    let fn_name_str = fn_name.to_string();

    let (params, return_type, call) = match &entry.kind {
        EventKind::Call {
            params_type,
            return_type,
        } => (
            quote! { plugin_name: String, params: #params_type },
            quote! { #return_type },
            quote! {
                use pyo3::IntoPyObject;
                pl_manager.bind(py).call_method1(
                    "call_event",
                    (plugin_name, #event_path, params.into_pyobject(py)?),
                )?
            },
        ),
        EventKind::Reduce { args, value_type } => {
            let names: Vec<_> = args.iter().map(|a| &a.name).collect();
            let types: Vec<_> = args.iter().map(|a| &a.ty).collect();
            (
                quote! { initial: #value_type, #(#names: #types),* },
                quote! { #value_type },
                quote! {
                    pl_manager.bind(py).call_method1(
                        "reduce_event",
                        (#event_path, initial, #(#names,)*),
                    )?
                },
            )
        }
    };

    quote! {
        #[napi]
        pub fn #fn_name(&self, #params) -> napi::Result<#return_type> {
            let pl_manager = &self.plmanager;
            pyo3::Python::attach(|py| -> pyo3::PyResult<#return_type> {
                let result = { #call };
                Ok(result.extract()?)
            })
            .map_err(|e| {
                napi::Error::from_reason(format!(
                    "Failed to call event '{}': {:?}",
                    #fn_name_str, e
                ))
            })
        }
    }
}

pub fn impl_plugin_event_methods(input: TokenStream) -> TokenStream {
    let EventInput {
        struct_name,
        entries,
    } = parse_macro_input!(input as EventInput);

    let methods: Vec<TokenStream2> = entries.iter().map(expand_entry).collect();

    quote! {
        #[napi]
        impl #struct_name {
            #(#methods)*
        }
    }
    .into()
}
