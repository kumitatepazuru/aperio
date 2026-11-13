// Registry の検証: 名前の形式、重複、Py と Cpu/Gpu の衝突、外部値の型の食い違いなど。

use std::sync::Arc;

use lua_gpu::spec::{
    CallSite, CodeGen, CpuFunction, ExternalSpec, FunctionSpec, Gen, GpuFunction, Options,
    PyFunction, Registry, SpecError,
};
use lua_gpu::types::TypeSpec;

fn gen() -> Arc<dyn CodeGen> {
    Arc::new(|_: &CallSite| Ok(Some(Gen::expr("0"))))
}

fn cpu(name: &str, externals: Vec<ExternalSpec>) -> FunctionSpec {
    FunctionSpec::Cpu(CpuFunction {
        name: name.into(),
        generate: gen(),
        returns: None,
        externals,
        property: false,
    })
}

fn gpu(name: &str, params: Vec<TypeSpec>) -> FunctionSpec {
    FunctionSpec::Gpu(GpuFunction {
        name: name.into(),
        generate: gen(),
        params,
        returns: vec![TypeSpec::Float],
        externals: vec![],
        property: false,
    })
}

fn py(name: &str) -> FunctionSpec {
    FunctionSpec::Py(PyFunction {
        name: name.into(),
        func: Arc::new(|_, args| Ok(args)),
        returns: None,
    })
}

#[test]
fn accepts_cpu_and_gpu_with_same_name() {
    let reg = Registry::new(
        vec![
            cpu("obj.getpixel", vec![]),
            gpu("obj.getpixel", vec![TypeSpec::Float, TypeSpec::Float]),
            py("debug_log"),
        ],
        Options::default(),
    )
    .unwrap();
    let e = reg.get("obj.getpixel").unwrap();
    assert!(e.cpu.is_some() && e.gpu.is_some() && e.py.is_none());
    assert_eq!(reg.root_names().collect::<Vec<_>>(), vec!["obj", "debug_log"]);
}

#[test]
fn rejects_bad_names() {
    for name in ["", "obj.", "1x", "obj.end", "a-b", ".x"] {
        let r = Registry::new(vec![cpu(name, vec![])], Options::default());
        assert!(
            matches!(r, Err(SpecError::InvalidName(_))),
            "`{name}` should be rejected"
        );
    }
}

#[test]
fn rejects_duplicates_and_py_conflicts() {
    assert!(matches!(
        Registry::new(vec![cpu("f", vec![]), cpu("f", vec![])], Options::default()),
        Err(SpecError::Duplicate { .. })
    ));
    assert!(matches!(
        Registry::new(vec![py("f"), gpu("f", vec![])], Options::default()),
        Err(SpecError::PyConflict(_))
    ));
}

#[test]
fn merges_externals_and_rejects_type_conflicts() {
    let reg = Registry::new(
        vec![
            cpu("a", vec![ExternalSpec::new("w", TypeSpec::Int)]),
            cpu("b", vec![ExternalSpec::new("w", TypeSpec::Int), ExternalSpec::new("t", TypeSpec::Float)]),
        ],
        Options::default(),
    )
    .unwrap();
    assert_eq!(reg.externals.keys().collect::<Vec<_>>(), vec!["w", "t"]);

    assert!(matches!(
        Registry::new(
            vec![
                cpu("a", vec![ExternalSpec::new("w", TypeSpec::Int)]),
                cpu("b", vec![ExternalSpec::new("w", TypeSpec::Float)]),
            ],
            Options::default(),
        ),
        Err(SpecError::ExternalConflict { .. })
    ));
}

#[test]
fn rejects_non_gpu_types_in_gpu_functions() {
    assert!(matches!(
        Registry::new(vec![gpu("f", vec![TypeSpec::String])], Options::default()),
        Err(SpecError::NonGpuType { .. })
    ));
}

#[test]
fn rejects_property_mismatch_between_cpu_and_gpu() {
    let mut c = cpu("obj.w", vec![]);
    if let FunctionSpec::Cpu(f) = &mut c {
        f.property = true;
    }
    let g = FunctionSpec::Gpu(GpuFunction {
        name: "obj.w".into(),
        generate: gen(),
        params: vec![],
        returns: vec![TypeSpec::Int],
        externals: vec![],
        property: false,
    });
    assert!(matches!(
        Registry::new(vec![c, g], Options::default()),
        Err(SpecError::PropertyConflict { .. })
    ));
}
