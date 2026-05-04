use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::Comma,
    Ident, Path, Result, Token, Type,
};

struct EventEntry {
    fn_name: Ident,
    event_path: Path,
    params_type: Type,
    return_type: Type,
}

impl Parse for EventEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let fn_name: Ident = input.parse()?;
        let content;
        syn::parenthesized!(content in input);
        let event_path: Path = content.parse()?;
        input.parse::<Token![:]>()?;
        let params_type: Type = input.parse()?;
        input.parse::<Token![->]>()?;
        let return_type: Type = input.parse()?;
        Ok(EventEntry {
            fn_name,
            event_path,
            params_type,
            return_type,
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

pub fn impl_plugin_event_methods(input: TokenStream) -> TokenStream {
    let EventInput {
        struct_name,
        entries,
    } = parse_macro_input!(input as EventInput);

    let methods: Vec<TokenStream2> = entries
        .iter()
        .map(|entry| {
            let fn_name = &entry.fn_name;
            let event_path = &entry.event_path;
            let params_type = &entry.params_type;
            let return_type = &entry.return_type;
            let fn_name_str = fn_name.to_string();

            quote! {
                #[napi]
                pub fn #fn_name(
                    &self,
                    plugin_name: String,
                    params: #params_type,
                ) -> napi::Result<#return_type> {
                    let pl_manager = &self.plmanager;
                    pyo3::Python::attach(|py| -> pyo3::PyResult<#return_type> {
                        let result = pl_manager
                            .bind(py)
                            .call_method1(
                                "call_event",
                                (plugin_name, #event_path, json_to_pyobject(py, &params)?),
                            )?;
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
        })
        .collect();

    quote! {
        #[napi]
        impl #struct_name {
            #(#methods)*
        }
    }
    .into()
}
