// 生成関数の呼び出し: CallSite の内容、呼び出し順、${} の置換、生成コードの取り込み、エラー処理。

mod support;

use std::sync::{Arc, Mutex};

use lua_gpu::analysis::consts::site_arg_info;
use lua_gpu::diag::Diagnostics;
use lua_gpu::expand::{self, SiteExpansion};
use lua_gpu::frontend;
use lua_gpu::spec::{
    CallSite, ConstValue, CpuFunction, ExternalSpec, FunctionSpec, Gen, Registry, SiteContext,
};
use lua_gpu::types::TypeSpec;
use support::*;

fn run(reg: &Registry, src: &str) -> (Vec<SiteExpansion>, Diagnostics, lua_gpu::hir::Program) {
    let mut diags = Diagnostics::new();
    let fe = frontend::build(src, "main.lua", None, reg, &mut diags).expect("parse");
    let mut program = fe.program;
    let info = site_arg_info(&program);
    let targets = expand::default_targets(&program, reg);
    let exps = expand::expand(&mut program, reg, &targets, &info, &mut diags);
    (exps, diags, program)
}

#[test]
fn passes_call_site_information_in_source_order() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let reg = registry(vec![
        FunctionSpec::Cpu(CpuFunction {
            name: "obj.getoption".into(),
            generate: recording(log.clone(), |s: &CallSite| {
                Some(Gen::expr(format!("${{track}}[{} + 1]", s.args[1])))
            }),
            returns: Some(tys(&["float"])),
            externals: vec![ExternalSpec::new("track", ty("float[4]"))],
            property: false,
        }),
        gpu_fn(
            "obj.getpixel",
            &["float", "float"],
            &["float"],
            recording(log.clone(), |s: &CallSite| {
                Some(Gen::expr(format!("float({} + {})", s.args[0], s.args[1])))
            }),
        ),
    ]);
    let src = "local k = 2\nlocal a = obj.getoption('track', k)\nlocal b = obj:getoption('x', a)\nlocal c = obj.getpixel(1, k * 3)";
    let (exps, diags, program) = run(&reg, src);
    assert!(!diags.has_errors(), "{diags}");

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].name, "obj.getoption");
    assert_eq!(log[0].args, vec!["a1", "a2"]);
    assert_eq!(
        log[0].consts,
        vec![Some(ConstValue::String("track".into())), Some(ConstValue::Number(2.0))]
    );
    assert_eq!(log[0].types, vec!["string", "int"]);
    assert_eq!((log[0].line, log[0].context), (2, SiteContext::Cpu));
    assert!(log[1].is_method);
    assert_eq!(log[1].consts[1], None);
    // GPU の site は Slang の仮引数名と宣言型が渡り、定数は伝播される。
    assert_eq!(log[2].context, SiteContext::Kernel);
    assert_eq!(log[2].types, vec!["float", "float"]);
    assert_eq!(log[2].consts[1], Some(ConstValue::Number(6.0)));

    let SiteExpansion::Cpu(c) = &exps[0] else { panic!() };
    assert_eq!(c.source, "function(a1, a2)\nreturn (__lg_ext.track[a2 + 1])\nend");
    assert_eq!(c.externals, vec!["track"]);
    // 生成コードは HIR にも取り込まれる。
    assert_eq!(program.func(c.func).params.len(), 2);
    let SiteExpansion::Gpu(g) = &exps[2] else { panic!() };
    assert_eq!(g.gen.exprs, vec!["float(a1 + a2)"]);
}

#[test]
fn substitutes_externals_and_escapes() {
    let mut used = Vec::new();
    let s = expand::substitute_externals("${w} * ${ h } + $${x} + $y", &["w", "h"], &mut used, |n| {
        format!("E.{n}")
    })
    .unwrap();
    assert_eq!(s, "E.w * E.h + ${x} + $y");
    assert_eq!(used, vec!["w", "h"]);
    assert!(expand::substitute_externals("${zz}", &["w"], &mut used, |n| n.into()).is_err());
    assert!(expand::substitute_externals("${w", &["w"], &mut used, |n| n.into()).is_err());
}

#[test]
fn gpu_externals_use_slang_names() {
    let reg = registry(vec![lua_gpu::spec::FunctionSpec::Gpu(lua_gpu::spec::GpuFunction {
        name: "now".into(),
        generate: fixed(Gen::expr("${time} * 2.0")),
        params: vec![],
        returns: vec![TypeSpec::Float],
        externals: vec![ExternalSpec::new("time", TypeSpec::Float)],
        property: true,
    })]);
    let (exps, diags, _) = run(&reg, "local t = now");
    assert!(!diags.has_errors(), "{diags}");
    let SiteExpansion::Gpu(g) = &exps[0] else { panic!() };
    assert_eq!(g.gen.exprs, vec!["lg_ext_time * 2.0"]);
}

#[test]
fn generator_failures_become_diagnostics() {
    let reg = registry(vec![
        cpu_fn("none", Arc::new(|_: &CallSite| Ok(None))),
        cpu_fn("err", Arc::new(|_: &CallSite| Err("boom".to_string()))),
        cpu_fn("bad_lua", fixed(Gen::expr("1 +"))),
        cpu_fn("undeclared", fixed(Gen::expr("${nope}"))),
        gpu_fn("g2", &["float", "float"], &["float"], fixed(Gen::expr("a1"))),
        gpu_fn("g0", &[], &["float", "float"], fixed(Gen::expr("1.0"))),
    ]);
    let (exps, diags, _) = run(
        &reg,
        "none()\nerr()\nbad_lua()\nundeclared()\nlocal a = g2(1)\nlocal b = g2(1, f())\nlocal c = g0()",
    );
    let codes: Vec<_> = diags.errors().map(|e| (e.code, e.location.line)).collect();
    assert_eq!(
        codes,
        vec![
            ("generator-failed", 1),
            ("generator-failed", 2),
            ("generator-syntax", 3),
            ("generator-external", 4),
            ("gpu-arity", 5),
            ("gpu-multi-arg", 6),
            ("generator-arity", 7),
        ]
    );
    assert!(matches!(exps[0], SiteExpansion::Failed));
    assert!(diags.errors().nth(1).unwrap().message.contains("boom"));
}
