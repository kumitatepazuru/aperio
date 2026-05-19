use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashSet;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{bracketed, parse_macro_input, Data, DeriveInput, Fields, Ident, Token};

mod plugin_events;

struct ApplyPartialArgs {
    target: Ident,
    skip: Vec<Ident>,
}

impl Parse for ApplyPartialArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut target = None;
        let mut skip = vec![];

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if key == "target" {
                input.parse::<Token![=]>()?;
                target = Some(input.parse::<Ident>()?);
            } else if key == "skip" {
                input.parse::<Token![=]>()?;
                let content;
                bracketed!(content in input);
                let ids = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                skip = ids.into_iter().collect();
            } else {
                return Err(syn::Error::new(key.span(), format!("unknown key: {}", key)));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ApplyPartialArgs {
            target: target.ok_or_else(|| input.error("target = Type is required"))?,
            skip,
        })
    }
}

#[proc_macro_derive(ApplyPartial, attributes(apply_partial))]
pub fn derive_apply_partial(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = input
        .attrs
        .iter()
        .find(|a| a.path().is_ident("apply_partial"))
        .map(|a| a.parse_args::<ApplyPartialArgs>().expect("invalid #[apply_partial(...)]"))
        .expect("ApplyPartial requires #[apply_partial(target = ..., skip = [...])]");

    let target_type = &args.target;
    let skip_set: HashSet<String> = args.skip.iter().map(|i| i.to_string()).collect();

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("ApplyPartial requires named fields"),
        },
        _ => panic!("ApplyPartial only supports structs"),
    };

    let stmts: Vec<_> = fields
        .iter()
        .filter_map(|f| {
            let name = f.ident.as_ref().unwrap();
            if skip_set.contains(&name.to_string()) {
                None
            } else {
                Some(quote! { if let Some(v) = self.#name.take() { s.#name = v; } })
            }
        })
        .collect();

    let struct_name = &input.ident;
    quote! {
        impl #struct_name {
            pub fn apply_to(&mut self, s: &mut #target_type) {
                #(#stmts)*
            }
        }
    }
    .into()
}

#[proc_macro]
pub fn impl_plugin_event_methods(input: TokenStream) -> TokenStream {
    plugin_events::impl_plugin_event_methods(input)
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
