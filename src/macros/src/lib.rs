use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, LitStr, Token};

struct MacroArgs {
    stub: bool,
    module: Option<String>,
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut stub = false;
        let mut module = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            if ident == "stub" {
                stub = true;
            } else if ident == "module" {
                input.parse::<Token![=]>()?;
                let lit: LitStr = input.parse()?;
                module = Some(lit.value());
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unknown argument: {}", ident),
                ));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(MacroArgs { stub, module })
    }
}

#[proc_macro_attribute]
pub fn pydataclass(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as MacroArgs);
    let input = parse_macro_input!(item as DeriveInput);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("pydataclass requires named fields"),
        },
        _ => panic!("pydataclass only supports structs"),
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

    let pyclass_attr = if let Some(ref module_str) = args.module {
        quote! { #[pyo3::pyclass(get_all, eq, module = #module_str)] }
    } else {
        quote! { #[pyo3::pyclass(get_all, eq)] }
    };

    let stub_struct_attr = if args.stub {
        quote! { #[pyo3_stub_gen::derive::gen_stub_pyclass] }
    } else {
        quote! {}
    };

    let stub_impl_attr = if args.stub {
        quote! { #[pyo3_stub_gen::derive::gen_stub_pymethods] }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #(#attrs)*
        #stub_struct_attr
        #pyclass_attr
        #[derive(Clone, PartialEq, Debug)]
        #vis struct #name {
            #( pub #field_names: #field_types, )*
        }

        #stub_impl_attr
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
    };

    expanded.into()
}
