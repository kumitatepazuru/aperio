use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Item, ItemMod};

/// pyo3の既知の問題により、from A.B import C 形式でサブモジュールを定義した場合、A.Bがsys.modulesに登録されないため、
/// このマクロを使ってサブモジュールを定義したモジュールをsys.modulesに登録するための #[register_submodules] マクロを定義する。
/// これをモジュール定義に付与すると、そのモジュール内のサブモジュールもすべてsys.modulesに登録されるようになる。
/// 
/// see: https://github.com/PyO3/pyo3/issues/759, https://github.com/PyO3/pyo3/issues/3900
pub fn register_submodules(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);

    let (brace_token, items) = match &module.content {
        Some(c) => c,
        // `mod name;` 形式（中身なし）はそのまま返す
        None => return TokenStream::from(quote! { #module }),
    };
    let _ = brace_token;

    // #[pymodule_init] が既にある場合は何もしない
    let already_has_init = items.iter().any(|item| {
        if let Item::Fn(f) = item {
            f.attrs.iter().any(|a| {
                a.path()
                    .segments
                    .last()
                    .map(|s| s.ident == "pymodule_init")
                    .unwrap_or(false)
            })
        } else {
            false
        }
    });

    if already_has_init {
        return TokenStream::from(quote! { #module });
    }

    let attrs = &module.attrs;
    let vis = &module.vis;
    let ident = &module.ident;

    // サブモジュールを sys.modules に登録する #[pymodule_init] を生成する。
    // m.dict() をイテレートして PyModule 型のエントリをすべて登録する。
    TokenStream::from(quote! {
        #(#attrs)*
        #vis mod #ident {
            #(#items)*

            #[pymodule_init]
            fn _register_submodules_init(
                m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>,
            ) -> ::pyo3::PyResult<()> {
                #[allow(unused_imports)]
                use ::pyo3::types::{PyAnyMethods, PyDictMethods, PyModuleMethods};

                let py = m.py();
                let sys = py.import("sys")?;
                let sys_modules = sys.getattr("modules")?;
                let parent_name: String = m.getattr("__name__")?.extract()?;
                for (key, val) in m.dict().iter() {
                    if val.is_instance_of::<::pyo3::types::PyModule>() {
                        let submod_name: String = key.extract()?;
                        sys_modules.set_item(
                            ::std::format!("{}.{}", parent_name, submod_name),
                            &val,
                        )?;
                    }
                }
                ::pyo3::PyResult::Ok(())
            }
        }
    })
}
