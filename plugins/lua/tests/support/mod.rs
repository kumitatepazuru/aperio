// 結合テスト共通のヘルパー(spec の組み立てなど)。
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use lua_gpu::spec::{
    CallSite, CodeGen, CpuFunction, ExternalSpec, FunctionSpec, Gen, GpuFunction, Options,
    PyFunction, Registry,
};
use lua_gpu::types::TypeSpec;

pub fn ty(s: &str) -> TypeSpec {
    s.parse().unwrap()
}

pub fn tys(list: &[&str]) -> Vec<TypeSpec> {
    list.iter().map(|s| ty(s)).collect()
}

/// 常に同じコードを返す生成関数。
pub fn fixed(gen: Gen) -> Arc<dyn CodeGen> {
    Arc::new(move |_: &CallSite| Ok(Some(gen.clone())))
}

/// 呼ばれた CallSite を記録しつつ、関数でコードを作る生成関数。
pub fn recording<F>(log: Arc<Mutex<Vec<CallSite>>>, f: F) -> Arc<dyn CodeGen>
where
    F: Fn(&CallSite) -> Option<Gen> + Send + Sync + 'static,
{
    Arc::new(move |site: &CallSite| {
        log.lock().unwrap().push(site.clone());
        Ok(f(site))
    })
}

pub fn cpu_fn(name: &str, gen: Arc<dyn CodeGen>) -> FunctionSpec {
    FunctionSpec::Cpu(CpuFunction {
        name: name.into(),
        generate: gen,
        returns: None,
        externals: vec![],
        property: false,
    })
}

pub fn cpu_prop(name: &str, gen: Arc<dyn CodeGen>, externals: Vec<ExternalSpec>) -> FunctionSpec {
    FunctionSpec::Cpu(CpuFunction {
        name: name.into(),
        generate: gen,
        returns: None,
        externals,
        property: true,
    })
}

pub fn gpu_fn(name: &str, params: &[&str], returns: &[&str], gen: Arc<dyn CodeGen>) -> FunctionSpec {
    FunctionSpec::Gpu(GpuFunction {
        name: name.into(),
        generate: gen,
        params: tys(params),
        returns: tys(returns),
        externals: vec![],
        property: false,
    })
}

pub fn py_fn(name: &str) -> FunctionSpec {
    FunctionSpec::Py(PyFunction {
        name: name.into(),
        func: Arc::new(|_, args| Ok(args)),
        returns: None,
    })
}

pub fn registry(functions: Vec<FunctionSpec>) -> Registry {
    Registry::new(functions, Options::default()).unwrap()
}

/// フロントエンド〜カーネル化の計画までを実行する。
pub fn analyze(reg: &Registry, src: &str) -> (Option<lua_gpu::pipeline::Analysis>, lua_gpu::diag::Diagnostics) {
    let mut diags = lua_gpu::diag::Diagnostics::new();
    let a = lua_gpu::pipeline::analyze(src, "main.lua", None, reg, &mut diags);
    (a, diags)
}

/// エラーなしで解析できることを確かめて返す。
pub fn analyze_ok(reg: &Registry, src: &str) -> lua_gpu::pipeline::Analysis {
    let (a, d) = analyze(reg, src);
    assert!(!d.has_errors(), "unexpected errors:\n{d}");
    a.expect("analysis")
}

pub fn error_codes(reg: &Registry, src: &str) -> Vec<&'static str> {
    let (_, d) = analyze(reg, src);
    d.errors().map(|e| e.code).collect()
}

pub fn warning_codes(reg: &Registry, src: &str) -> Vec<&'static str> {
    let (_, d) = analyze(reg, src);
    d.warnings().map(|e| e.code).collect()
}

/// 典型的な obj API(テスト用): getpixel / putpixel(GPU)、w / h(外部値を読む CPU property)。
pub fn obj_registry() -> Registry {
    registry(vec![
        gpu_fn(
            "obj.getpixel",
            &["float", "float"],
            &["float", "float", "float", "float"],
            fixed(Gen {
                exprs: vec!["0.0".into(), "0.0".into(), "0.0".into(), "1.0".into()],
                ..Gen::default()
            }),
        ),
        gpu_fn(
            "obj.putpixel",
            &["float", "float", "float", "float", "float", "float"],
            &[],
            fixed(Gen::default()),
        ),
        cpu_prop(
            "obj.w",
            fixed(Gen::expr("${w}")),
            vec![ExternalSpec::new("w", TypeSpec::Int)],
        ),
        cpu_prop(
            "obj.h",
            fixed(Gen::expr("${h}")),
            vec![ExternalSpec::new("h", TypeSpec::Int)],
        ),
        py_fn("debug_log"),
    ])
}
