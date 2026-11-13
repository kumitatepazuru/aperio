//! 独自関数の呼び出し箇所ごとに生成関数を呼び、返ってきたコードを取り込む。
//!
//! - CpuFunction: `function(a1, ..., an) <stmts> return <exprs> end` という Lua 関数にして、
//!   HIR にも lowering する(カーネル内で使われたときに解析・Slang 化するため)。
//! - GpuFunction: Slang のコード片(Gen)をそのまま保持する。Slang の関数化は codegen で行う。
//! - 生成コード中の `${name}` は外部値の参照に置き換える(`$${` は `${` そのもの)。

use crate::analysis::consts::SiteArgInfo;
use crate::diag::Diagnostics;
use crate::frontend::{self, Lowerer};
use crate::hir::*;
use crate::spec::{CallSite, Gen, Registry, SiteContext};
use crate::types::TypeSpec;

/// 各 site にどの実装を使うか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteTarget {
    Cpu,
    Gpu,
    Py,
}

#[derive(Clone, Debug)]
pub struct CpuExpansion {
    /// `function(a1, ...) ... end` の Lua ソース(外部値は `__lg_ext.name` に置換済み)。
    pub source: String,
    /// 生成関数の HIR。
    pub func: FuncId,
    pub externals: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GpuExpansion {
    /// 外部値を `lg_ext_<name>` に置換済みの Slang コード。
    pub gen: Gen,
    pub externals: Vec<String>,
    pub params: Vec<TypeSpec>,
    pub returns: Vec<TypeSpec>,
}

#[derive(Clone, Debug)]
pub enum SiteExpansion {
    Cpu(CpuExpansion),
    Gpu(GpuExpansion),
    Py,
    /// 生成に失敗した(診断を出し済み)。
    Failed,
}

/// spec の種類だけから決める既定の割り当て(Gpu だけ登録 → Gpu、Py → Py、それ以外 → Cpu)。
pub fn default_targets(program: &Program, registry: &Registry) -> Vec<SiteTarget> {
    program
        .sites
        .iter()
        .map(|s| {
            let e = registry.get(&s.name).expect("resolved site must exist in registry");
            if e.py.is_some() {
                SiteTarget::Py
            } else if e.cpu.is_none() {
                SiteTarget::Gpu
            } else {
                SiteTarget::Cpu
            }
        })
        .collect()
}

/// 生成コード中の Slang の仮引数名。
pub fn slang_param_name(i: usize) -> String {
    format!("a{}", i + 1)
}

/// Slang で外部値を参照する名前。
pub fn slang_external_name(name: &str) -> String {
    format!("lg_ext_{name}")
}

pub fn expand(
    program: &mut Program,
    registry: &Registry,
    targets: &[SiteTarget],
    arg_info: &[SiteArgInfo],
    diags: &mut Diagnostics,
) -> Vec<SiteExpansion> {
    let mut out = Vec::with_capacity(program.sites.len());
    // 生成コードは spec を呼べない(PyFunction は実体があるので普通のグローバル関数として見える)。
    let plain_registry =
        Registry::new(Vec::new(), registry.options.clone()).expect("empty registry is valid");

    for id in 0..program.sites.len() {
        let site = program.sites[id].clone();
        let entry = registry.get(&site.name).expect("site name in registry").clone();
        let loc = program.location(site.span);
        let info = arg_info.get(id).cloned().unwrap_or_default();
        let target = targets[id];

        let exp = match target {
            SiteTarget::Py => SiteExpansion::Py,
            SiteTarget::Cpu => {
                let spec = registry.cpu(&entry).expect("cpu target requires a CpuFunction");
                let args: Vec<String> = (0..site.args.len()).map(|i| format!("a{}", i + 1)).collect();
                let call = CallSite {
                    name: site.name.clone(),
                    args: args.clone(),
                    consts: info.consts.clone(),
                    types: info.types.clone(),
                    is_method: site.is_method,
                    chunk: loc.chunk.clone(),
                    line: loc.line,
                    col: loc.col,
                    context: SiteContext::Cpu,
                };
                match run_generator(spec.generate.as_ref(), &call) {
                    Err(msg) => {
                        diags.error("generator-failed", loc, format!("`{}`: {msg}", site.name));
                        SiteExpansion::Failed
                    }
                    Ok(gen) => {
                        if let Some(r) = &spec.returns {
                            if r.len() != gen.exprs.len() {
                                diags.error(
                                    "generator-arity",
                                    loc.clone(),
                                    format!(
                                        "`{}`: generator returned {} expression(s) but `returns` declares {}",
                                        site.name,
                                        gen.exprs.len(),
                                        r.len()
                                    ),
                                );
                            }
                        }
                        let declared: Vec<&str> =
                            spec.externals.iter().map(|e| e.name.as_str()).collect();
                        let mut externals = Vec::new();
                        let mut subst = |code: &str, diags: &mut Diagnostics| {
                            substitute_externals(code, &declared, &mut externals, |n| {
                                format!("__lg_ext.{n}")
                            })
                            .map_err(|m| diags.error("generator-external", loc.clone(), format!("`{}`: {m}", site.name)))
                            .ok()
                        };
                        let stmts = subst(&gen.stmts, diags);
                        let exprs: Option<Vec<String>> =
                            gen.exprs.iter().map(|e| subst(e, diags)).collect();
                        match (stmts, exprs) {
                            (Some(stmts), Some(exprs)) => {
                                let source = lua_function_source(&args, &stmts, &exprs);
                                match lower_generated(program, &plain_registry, &source, &site, diags) {
                                    Some(func) => SiteExpansion::Cpu(CpuExpansion {
                                        source,
                                        func,
                                        externals,
                                    }),
                                    None => SiteExpansion::Failed,
                                }
                            }
                            _ => SiteExpansion::Failed,
                        }
                    }
                }
            }
            SiteTarget::Gpu => {
                let spec = registry.gpu(&entry).expect("gpu target requires a GpuFunction");
                if !site.property && site.args.len() != spec.params.len() {
                    diags.error(
                        "gpu-arity",
                        loc.clone(),
                        format!(
                            "`{}` takes {} argument(s) but {} were given",
                            site.name,
                            spec.params.len(),
                            site.args.len()
                        ),
                    );
                }
                if site.args.last().is_some_and(|a| a.is_multi(program)) {
                    diags.error(
                        "gpu-multi-arg",
                        program.location(site.args.last().unwrap().span),
                        format!(
                            "the last argument of GPU function `{}` must be a single value \
                             (wrap it in parentheses)",
                            site.name
                        ),
                    );
                }
                let call = CallSite {
                    name: site.name.clone(),
                    args: (0..spec.params.len()).map(slang_param_name).collect(),
                    consts: info.consts.clone(),
                    types: spec.params.iter().map(|t| t.to_string()).collect(),
                    is_method: site.is_method,
                    chunk: loc.chunk.clone(),
                    line: loc.line,
                    col: loc.col,
                    context: SiteContext::Kernel,
                };
                match run_generator(spec.generate.as_ref(), &call) {
                    Err(msg) => {
                        diags.error("generator-failed", loc, format!("`{}`: {msg}", site.name));
                        SiteExpansion::Failed
                    }
                    Ok(gen) => {
                        if gen.exprs.len() != spec.returns.len() {
                            diags.error(
                                "generator-arity",
                                loc.clone(),
                                format!(
                                    "`{}`: generator returned {} expression(s) but `returns` declares {}",
                                    site.name,
                                    gen.exprs.len(),
                                    spec.returns.len()
                                ),
                            );
                        }
                        let declared: Vec<&str> =
                            spec.externals.iter().map(|e| e.name.as_str()).collect();
                        let mut externals = Vec::new();
                        let mut ok = true;
                        let mut sub = |code: &str| match substitute_externals(
                            code,
                            &declared,
                            &mut externals,
                            slang_external_name,
                        ) {
                            Ok(s) => s,
                            Err(m) => {
                                diags.error(
                                    "generator-external",
                                    loc.clone(),
                                    format!("`{}`: {m}", site.name),
                                );
                                ok = false;
                                String::new()
                            }
                        };
                        let gen = Gen {
                            exprs: gen.exprs.iter().map(|e| sub(e)).collect(),
                            stmts: sub(&gen.stmts),
                            defs: sub(&gen.defs),
                        };
                        if ok {
                            SiteExpansion::Gpu(GpuExpansion {
                                gen,
                                externals,
                                params: spec.params.clone(),
                                returns: spec.returns.clone(),
                            })
                        } else {
                            SiteExpansion::Failed
                        }
                    }
                }
            }
        };
        out.push(exp);
    }
    out
}

fn run_generator(gen: &dyn crate::spec::CodeGen, site: &CallSite) -> Result<Gen, String> {
    match gen.generate(site) {
        Ok(Some(g)) => Ok(g),
        Ok(None) => Err("generator returned None (not convertible here)".to_string()),
        Err(e) => Err(e),
    }
}

fn lua_function_source(args: &[String], stmts: &str, exprs: &[String]) -> String {
    let mut s = format!("function({})\n", args.join(", "));
    if !stmts.trim().is_empty() {
        s.push_str(stmts);
        s.push('\n');
    }
    if !exprs.is_empty() {
        // 各式を括弧で囲み、多値や優先順位の事故を防ぐ。
        let es: Vec<String> = exprs.iter().map(|e| format!("({e})")).collect();
        s.push_str(&format!("return {}\n", es.join(", ")));
    }
    s.push_str("end");
    s
}

/// 生成された Lua 関数を HIR に lowering し、その関数の FuncId を返す。
fn lower_generated(
    program: &mut Program,
    plain_registry: &Registry,
    func_source: &str,
    site: &Site,
    diags: &mut Diagnostics,
) -> Option<FuncId> {
    let loc = program.location(site.span);
    let chunk_source = format!("return {func_source}");
    let ast = match frontend::parse(&chunk_source) {
        Ok(a) => a,
        Err(errors) => {
            let msgs: Vec<String> = errors.iter().map(|e| e.error_message().into_owned()).collect();
            diags.error(
                "generator-syntax",
                loc,
                format!(
                    "`{}`: generated Lua does not parse ({}):\n{func_source}",
                    site.name,
                    msgs.join("; ")
                ),
            );
            return None;
        }
    };
    let file = program.files.len() as FileId;
    program.files.push(SourceFile {
        id: file,
        chunk: format!("={}@{}:{}", site.name, loc.chunk, loc.line),
        path: None,
        source: chunk_source.clone(),
        kind: SourceKind::Generated,
        main: 0,
    });
    let span = Span {
        file,
        start: 0,
        end: chunk_source.len() as u32,
        line: 1,
        col: 1,
    };
    let main = {
        let mut lw = Lowerer::new(program, plain_registry, diags, file);
        lw.lower_chunk(ast.nodes(), span)
    };
    program.files[file as usize].main = main;
    match program.func(main).body.stmts.first().map(|s| &s.kind) {
        Some(StmtKind::Return(r)) => match r.first().map(|e| &e.kind) {
            Some(ExprKind::Function(f)) => Some(*f),
            _ => None,
        },
        _ => None,
    }
}

/// `${name}` を `render(name)` に置き換える。宣言されていない外部値はエラー。`$${` は `${`。
pub fn substitute_externals(
    code: &str,
    declared: &[&str],
    used: &mut Vec<String>,
    render: impl Fn(&str) -> String,
) -> Result<String, String> {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        if let Some(r) = after.strip_prefix("$${") {
            out.push_str("${");
            rest = r;
        } else if let Some(r) = after.strip_prefix("${") {
            let end = r
                .find('}')
                .ok_or_else(|| "unterminated `${` in generated code".to_string())?;
            let name = r[..end].trim();
            if !declared.contains(&name) {
                return Err(format!(
                    "external `{name}` is not declared in this function's `externals`"
                ));
            }
            if !used.iter().any(|u| u == name) {
                used.push(name.to_string());
            }
            out.push_str(&render(name));
            rest = &r[end + 1..];
        } else {
            out.push('$');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    Ok(out)
}
