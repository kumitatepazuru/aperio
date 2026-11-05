//! カーネル(HIR + 計画)から Slang のコンピュートシェーダーを生成する。
//!
//! - 入出力はユーザー指定の `EffectResourcesSampler` と同じ形を生成コード内に直接書く(自己完結)。
//! - params は `StructuredBuffer<LuaGpuWord>`(32 bit ワード列)として読む([`super::layout`])。
//! - 反復空間をスレッドに grid-stride で割り当てる。i = (外側の反復番号) * (内側の回数) + (内側の反復番号)。
//! - Lua の number は float。テーブルは、CPU から渡すもの(配列・レコード)を生成した構造体で、
//!   カーネル内で作る配列を固定長配列 + 長さで表す。

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use indexmap::IndexMap;

use crate::analysis::resolve::{Resolver, StaticValue};
use crate::codegen::layout::{Field, KernelLayout};
use crate::diag::Diagnostics;
use crate::expand::{SiteExpansion, SiteTarget};
use crate::hir::*;
use crate::kernel::types::{ArrLen, GpuTy};
use crate::kernel::*;
use crate::pipeline::Analysis;
use crate::spec::Registry;
use crate::stdlib::{Builtin, GpuImpl};

pub const HELPERS: &str = include_str!("helpers.slang");

/// ディスパッチ用ブロックが params に並べる値(トップレベルのフィールド)の意味。
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// 反復の総数。
    Total,
    DimStart(usize),
    DimStep(usize),
    DimCount(usize),
    Capture(CaptureSource),
    Hoist(usize),
    Records,
}

#[derive(Clone, Debug)]
pub struct SlangKernel {
    pub source: String,
    pub layout: KernelLayout,
    pub entries: Vec<Entry>,
}

pub fn generate(
    a: &Analysis,
    k: &Kernel,
    registry: &Registry,
    diags: &mut Diagnostics,
) -> Option<SlangKernel> {
    let resolver = Resolver::new(&a.program);
    let g = Gen {
        a,
        k,
        registry,
        resolver: &resolver,
        diags,
        out: String::new(),
        indent: 1,
        tmp: 0,
        types: TypeReg::default(),
        defs: Vec::new(),
        site_fns: IndexMap::new(),
        local_arrays: HashMap::new(),
        loop_labels: Vec::new(),
        in_func: None,
        errors: false,
        cap_names: HashMap::new(),
        str_consts: Vec::new(),
    };
    g.run()
}

#[derive(Default)]
struct TypeReg {
    /// 型のキー → (Slang の型名)。
    names: HashMap<String, String>,
    defs: Vec<String>,
}

struct Gen<'a> {
    a: &'a Analysis,
    k: &'a Kernel,
    registry: &'a Registry,
    resolver: &'a Resolver<'a>,
    diags: &'a mut Diagnostics,
    out: String,
    indent: usize,
    tmp: u32,
    types: TypeReg,
    /// Gen.defs(内容で重複を除く)。
    defs: Vec<String>,
    site_fns: IndexMap<SiteId, String>,
    /// カーネル内で作る配列(local の VarId → (要素型, 容量))。
    local_arrays: HashMap<VarId, (GpuTy, u32)>,
    /// 囲んでいるループの本体の末尾にあるラベル(`goto continue` の変換用)。
    loop_labels: Vec<Option<String>>,
    in_func: Option<FuncId>,
    errors: bool,
    /// 捕捉した値の Slang 名(CaptureSource のキー → 名前)。
    cap_names: HashMap<String, String>,
    str_consts: Vec<String>,
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn float_lit(v: f64) -> String {
    let f = v as f32;
    if f.is_nan() {
        "asfloat(0x7fc00000u)".into()
    } else if f.is_infinite() {
        if f > 0.0 {
            "(1.0 / 0.0)".into()
        } else {
            "(-1.0 / 0.0)".into()
        }
    } else {
        let s = format!("{f:?}");
        if s.contains('.') || s.contains('e') || s.contains("inf") {
            s
        } else {
            format!("{s}.0")
        }
    }
}

fn capture_key(src: &CaptureSource) -> String {
    match src {
        CaptureSource::Var(VarRef::Local(v)) => format!("local:{v}"),
        CaptureSource::Var(VarRef::Global(g)) => format!("global:{g}"),
        CaptureSource::External(n) => format!("ext:{n}"),
    }
}

impl<'a> Gen<'a> {
    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.errors = true;
        let loc = self.a.program.location(span);
        self.diags.error("slang-codegen", loc, msg);
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn fresh(&mut self, base: &str) -> String {
        self.tmp += 1;
        format!("lg_{base}{}", self.tmp)
    }

    fn var_name(&self, v: VarId) -> String {
        format!("l{}_{}", v, sanitize(&self.a.program.var(v).name))
    }

    // ---- 型 ---------------------------------------------------------------------

    fn ty_key(ty: &GpuTy) -> String {
        ty.describe()
    }

    /// Slang の型名(構造体が必要なら定義も登録する)。
    fn slang_type(&mut self, ty: &GpuTy) -> String {
        match ty {
            GpuTy::Float | GpuTy::Unknown => "float".into(),
            GpuTy::Bool => "bool".into(),
            GpuTy::Array(elem, _) => {
                let key = Self::ty_key(&GpuTy::Array(elem.clone(), ArrLen::Param));
                if let Some(n) = self.types.names.get(&key) {
                    return n.clone();
                }
                let name = format!("LgArr{}", self.types.names.len());
                self.types.names.insert(key, name.clone());
                let et = self.slang_type(elem);
                let stride = Field::from_ty(elem).map(|f| f.words()).unwrap_or(1);
                let load = self.elem_load(elem, "(off + uint(i) * STRIDE)").replace("STRIDE", &format!("{stride}u"));
                let default = self.default_value(elem);
                let def = format!(
                    "struct {name} {{\n    uint off;\n    uint len;\n    __subscript(int i) -> {et} {{\n        get {{\n            if (i < 0 || uint(i) >= len) {{ return {default}; }}\n            return {load};\n        }}\n    }}\n}};\n{name} lg_load_{name}(uint at) {{ {name} a; a.off = lg_u(at); a.len = lg_u(at + 1u); return a; }}\n"
                );
                self.types.defs.push(def);
                name
            }
            GpuTy::Record(fields) => {
                let key = Self::ty_key(ty);
                if let Some(n) = self.types.names.get(&key) {
                    return n.clone();
                }
                let name = format!("LgRec{}", self.types.names.len());
                self.types.names.insert(key, name.clone());
                let mut body = String::new();
                let mut load = String::new();
                let mut at = 0u32;
                for (f, t) in fields {
                    let st = self.slang_type(t);
                    let fname = format!("f_{}", sanitize(f));
                    let _ = writeln!(body, "    {st} {fname};");
                    let words = Field::from_ty(t).map(|f| f.words()).unwrap_or(1);
                    let _ = writeln!(
                        load,
                        "    r.{fname} = {};",
                        self.elem_load(t, &format!("(at + {at}u)"))
                    );
                    at += words;
                }
                let def = format!(
                    "struct {name} {{\n{body}}};\n{name} lg_load_{name}(uint at) {{\n    {name} r;\n{load}    return r;\n}}\n"
                );
                self.types.defs.push(def);
                name
            }
            _ => "float".into(),
        }
    }

    /// ワード位置 `at`(Slang の式)から型 `ty` の値を読む式。
    fn elem_load(&mut self, ty: &GpuTy, at: &str) -> String {
        match ty {
            GpuTy::Float | GpuTy::Unknown => format!("lg_f({at})"),
            GpuTy::Bool => format!("lg_b({at})"),
            _ => {
                let n = self.slang_type(ty);
                format!("lg_load_{n}({at})")
            }
        }
    }

    fn default_value(&mut self, ty: &GpuTy) -> String {
        match ty {
            GpuTy::Float | GpuTy::Unknown => "0.0".into(),
            GpuTy::Bool => "false".into(),
            _ => {
                let n = self.slang_type(ty);
                format!("(({n})0)")
            }
        }
    }

    fn expr_ty(&self, e: &Expr) -> GpuTy {
        if let Some(r) = self.k.cpu_refs.get(&ExprKey::of(e)) {
            return match r {
                CpuRef::Hoist(i) => self.k.hoists[*i].ty.clone(),
                CpuRef::Record(i) => self.k.records[*i].ty.clone(),
            };
        }
        if let ExprKind::Local(v) = e.kind {
            if let Some(t) = self.k.var_types.get(&v) {
                return t.clone();
            }
            if self.k.dims.iter().any(|d| d.var == v) {
                return GpuTy::Float;
            }
        }
        if let ExprKind::Paren(inner) = &e.kind {
            return self.expr_ty(inner);
        }
        self.k
            .expr_types
            .get(&ExprKey::of(e))
            .cloned()
            .unwrap_or(GpuTy::Float)
    }

    // ---- 全体 ---------------------------------------------------------------------

    fn run(mut self) -> Option<SlangKernel> {
        let opts = &self.registry.options;
        // ---- レイアウト ----
        let mut entries = vec![Entry::Total];
        let mut fields = vec![Field::U32];
        for i in 0..self.k.dims.len() {
            entries.extend([Entry::DimStart(i), Entry::DimStep(i), Entry::DimCount(i)]);
            fields.extend([Field::F32, Field::F32, Field::U32]);
        }
        // 捕捉(GpuFunction の生成コードが参照する外部値も含める)。
        let mut captures: Vec<(CaptureSource, GpuTy, String)> = self
            .k
            .captures
            .values()
            .map(|c| {
                let name = match &c.source {
                    CaptureSource::External(n) => crate::expand::slang_external_name(n),
                    _ => c.name.clone(),
                };
                (c.source.clone(), c.ty.clone(), name)
            })
            .collect();
        for ext in self.site_externals() {
            if !captures
                .iter()
                .any(|(s, _, _)| *s == CaptureSource::External(ext.clone()))
            {
                let ty = self
                    .registry
                    .externals
                    .get(&ext)
                    .map(GpuTy::from_spec)
                    .unwrap_or(GpuTy::Float);
                if !ty.is_storable() {
                    let span = self.k.span;
                    self.error(span, format!("external `{ext}` of type {} cannot be used on the GPU", ty.describe()));
                    continue;
                }
                captures.push((
                    CaptureSource::External(ext.clone()),
                    ty,
                    crate::expand::slang_external_name(&ext),
                ));
            }
        }
        for (src, ty, name) in &captures {
            self.cap_names.insert(capture_key(src), name.clone());
            entries.push(Entry::Capture(src.clone()));
            match Field::from_ty(ty) {
                Some(f) => fields.push(f),
                None => {
                    let span = self.k.span;
                    self.error(span, format!("value `{name}` of type {} cannot be passed to the GPU", ty.describe()));
                    fields.push(Field::F32);
                }
            }
        }
        for (i, h) in self.k.hoists.iter().enumerate() {
            entries.push(Entry::Hoist(i));
            fields.push(if h.ty == GpuTy::Bool { Field::Bool } else { Field::F32 });
        }
        if self.k.slots_per_iteration > 0 {
            entries.push(Entry::Records);
            fields.push(Field::Records(self.k.slots_per_iteration));
        }
        let layout = KernelLayout { fields };
        let offsets = layout.offsets();

        // ---- カーネルで作る配列の容量 ----
        self.collect_local_arrays(opts.max_local_table_len);

        // ---- 初期化(捕捉・持ち出しを static に読み込む) ----
        let mut statics = String::new();
        let mut init = String::new();
        let mut entry_idx = 1 + 3 * self.k.dims.len();
        for (_, ty, name) in &captures {
            let st = self.slang_type(ty);
            let _ = writeln!(statics, "static {st} {name};");
            let load = self.elem_load(ty, &format!("{}u", offsets[entry_idx]));
            let _ = writeln!(init, "    {name} = {load};");
            entry_idx += 1;
        }
        for (i, h) in self.k.hoists.iter().enumerate() {
            let st = if h.ty == GpuTy::Bool { "bool" } else { "float" };
            let load = if h.ty == GpuTy::Bool { "lg_b" } else { "lg_f" };
            let _ = writeln!(statics, "static {st} lg_h{i};");
            let _ = writeln!(init, "    lg_h{i} = {load}({}u);", offsets[entry_idx]);
            entry_idx += 1;
        }
        if self.k.slots_per_iteration > 0 {
            let _ = writeln!(statics, "static uint lg_rec_base;");
            let _ = writeln!(init, "    lg_rec_base = lg_u({}u);", offsets[entry_idx]);
        }

        // ---- 関数(ユーザー関数・CpuFunction の生成関数) ----
        let mut funcs_src = String::new();
        for f in self.k.funcs.clone() {
            funcs_src.push_str(&self.function(f));
        }

        // ---- 本体 ----
        self.indent = 2;
        let body = self.k.body.clone();
        // 私有化した外側の変数は、反復ごとの local として宣言する。
        let mut privs: Vec<VarId> = self
            .k
            .privatized
            .iter()
            .filter_map(|v| match v {
                VarRef::Local(id) => Some(*id),
                _ => None,
            })
            .collect();
        privs.sort();
        for v in privs {
            let ty = self.k.var_types.get(&v).cloned().unwrap_or(GpuTy::Float);
            let st = self.slang_type(&ty);
            let d = self.default_value(&ty);
            let n = self.var_name(v);
            self.line(&format!("{st} {n} = {d};"));
        }
        self.loop_labels.push(end_label(&body));
        self.block_stmts(&body);
        self.loop_labels.pop();
        let body_src = std::mem::take(&mut self.out);

        if self.errors {
            return None;
        }

        // ---- 組み立て ----
        let mut s = String::new();
        let _ = writeln!(s, "// generated by lua_gpu: kernel {} ({})", self.k.id, self.location_text());
        s.push_str("struct LuaGpuWord { uint v; };\n");
        s.push_str(
            "public struct EffectResourcesSampler<Params> {\n    [vk::image_format(APERIO_IMAGE_FORMAT)]\n    public RWTexture2D<float4> outputTex;\n    public SamplerState samp;\n    public StructuredBuffer<Params> params;\n};\n",
        );
        s.push_str("struct LuaGpuInputs {\n    Texture2D<float4> tex0;\n");
        for input in &opts.extra_inputs {
            let _ = writeln!(s, "    Texture2D<float4> {input};");
        }
        s.push_str("};\n");
        s.push_str("[vk::binding(0, 0)]\nParameterBlock<LuaGpuInputs> inputs;\n");
        s.push_str("[vk::binding(0, 1)]\nParameterBlock<EffectResourcesSampler<LuaGpuWord>> res;\n\n");
        s.push_str(HELPERS);
        s.push('\n');
        for d in &self.types.defs {
            s.push_str(d);
        }
        for c in &self.str_consts {
            s.push_str(c);
        }
        for d in &self.defs {
            s.push_str(d);
            s.push('\n');
        }
        s.push_str("static uint lg_iter;\n");
        s.push_str(&statics);
        s.push_str("void lg_init() {\n");
        s.push_str(&init);
        s.push_str("}\n\n");
        for (_, f) in &self.site_fns {
            s.push_str(f);
        }
        s.push_str(&funcs_src);
        s.push_str("[shader(\"compute\")]\n[numthreads(16, 16, 1)]\nvoid main(uint3 gid : SV_DispatchThreadID) {\n");
        s.push_str("    uint W, H;\n    res.outputTex.GetDimensions(W, H);\n    if (gid.x >= W || gid.y >= H) return;\n    lg_init();\n");
        s.push_str("    uint lg_n = lg_u(0u);\n");
        s.push_str("    for (uint lg_i = gid.y * W + gid.x; lg_i < lg_n; lg_i += W * H) {\n");
        s.push_str("        lg_iter = lg_i;\n");
        if !self.k.dims.is_empty() {
            s.push_str("        uint lg_rest = lg_i;\n");
            for d in (0..self.k.dims.len()).rev() {
                let base = 1 + 3 * d;
                let var = self.var_name(self.k.dims[d].var);
                let _ = writeln!(
                    s,
                    "        uint lg_c{d} = lg_u({}u);\n        uint lg_k{d} = lg_rest % lg_c{d};\n        lg_rest = lg_rest / lg_c{d};\n        float {var} = lg_f({}u) + float(lg_k{d}) * lg_f({}u);",
                    offsets[base + 2],
                    offsets[base],
                    offsets[base + 1]
                );
            }
        }
        s.push_str(&body_src);
        s.push_str("    }\n}\n");
        Some(SlangKernel {
            source: s,
            layout,
            entries,
        })
    }

    fn location_text(&self) -> String {
        let loc = self.a.program.location(self.k.span);
        format!("{}:{}", loc.chunk, loc.line)
    }

    /// カーネルで使う GpuFunction の生成コードが参照する外部値。
    fn site_externals(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut visit = |b: &Block| {
            crate::kernel::deps::visit_reads(&self.a.program, b, &mut |e| {
                if let ExprKind::Spec(site) = e.kind {
                    if let SiteExpansion::Gpu(g) = &self.a.expansions[site as usize] {
                        for x in &g.externals {
                            if !out.contains(x) {
                                out.push(x.clone());
                            }
                        }
                    }
                }
            });
        };
        visit(&self.k.body);
        for f in &self.k.funcs {
            visit(&self.a.program.func(*f).body);
        }
        out
    }

    /// カーネル内で作る配列とその容量。添字で書き込まれる配列は上限まで伸ばせるようにする。
    fn collect_local_arrays(&mut self, max_len: u32) {
        let mut grow = HashSet::new();
        let mut scan = |b: &Block| {
            fn go(b: &Block, grow: &mut HashSet<VarId>) {
                for s in &b.stmts {
                    match &s.kind {
                        StmtKind::Assign { targets, .. } => {
                            for t in targets {
                                if let LValue::Index { obj, .. } = t {
                                    if let ExprKind::Local(v) = obj.kind {
                                        grow.insert(v);
                                    }
                                }
                            }
                        }
                        StmtKind::Do(b) => go(b, grow),
                        StmtKind::If { arms, else_block } => {
                            for (_, blk) in arms {
                                go(blk, grow);
                            }
                            if let Some(blk) = else_block {
                                go(blk, grow);
                            }
                        }
                        StmtKind::While { body, .. }
                        | StmtKind::Repeat { body, .. }
                        | StmtKind::NumFor { body, .. }
                        | StmtKind::GenFor { body, .. } => go(body, grow),
                        _ => {}
                    }
                }
            }
            go(b, &mut grow);
        };
        scan(&self.k.body);
        for f in &self.k.funcs {
            scan(&self.a.program.func(*f).body);
        }
        for (v, t) in &self.k.var_types {
            if let GpuTy::Array(elem, ArrLen::Local(n)) = t {
                let cap = if grow.contains(v) { (*n).max(max_len) } else { *n };
                self.local_arrays.insert(*v, ((**elem).clone(), cap.max(1)));
            }
        }
    }

    // ---- 関数 -------------------------------------------------------------------

    fn ret_type(&mut self, returns: &[GpuTy], struct_name: &str) -> String {
        match returns.len() {
            0 => "void".into(),
            1 => self.slang_type(&returns[0]),
            _ => {
                let mut def = format!("struct {struct_name} {{");
                for (i, t) in returns.iter().enumerate() {
                    let st = self.slang_type(t);
                    let _ = write!(def, " {st} r{i};");
                }
                def.push_str(" };\n");
                self.types.defs.push(def);
                struct_name.to_string()
            }
        }
    }

    fn function(&mut self, f: FuncId) -> String {
        let Some(sig) = self.k.func_types.get(&f).cloned() else {
            return String::new();
        };
        let func = self.a.program.func(f).clone();
        let ret = self.ret_type(&sig.returns, &format!("LgRet{f}"));
        let mut params = Vec::new();
        for (v, t) in func.params.iter().zip(sig.params.iter()) {
            let st = self.slang_type(t);
            params.push(format!("{st} {}", self.var_name(*v)));
        }
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        let saved_func = self.in_func.replace(f);
        let saved_labels = std::mem::take(&mut self.loop_labels);
        self.indent = 1;
        self.block_stmts(&func.body);
        if !sig.returns.is_empty() && !ends_with_return(&func.body) {
            // Lua では末尾まで来ると何も返さない(nil)。GPU では既定値を返す。
            let mut vals = Vec::new();
            for t in &sig.returns {
                vals.push(self.default_value(t));
            }
            let r = self.make_return(&vals, &sig.returns, f);
            self.line(&r);
        }
        let body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        self.in_func = saved_func;
        self.loop_labels = saved_labels;
        let name = func.name.clone().unwrap_or_default();
        format!(
            "// {name}\n{ret} lg_fn{f}({}) {{\n{body}}}\n\n",
            params.join(", ")
        )
    }

    fn make_return(&mut self, vals: &[String], types: &[GpuTy], f: FuncId) -> String {
        match vals.len() {
            0 => "return;".into(),
            1 => format!("return {};", vals[0]),
            _ => {
                let _ = types;
                let t = format!("LgRet{f}");
                let fields: Vec<String> = vals
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!("lg_r.r{i} = {v};"))
                    .collect();
                format!("{{ {t} lg_r; {} return lg_r; }}", fields.join(" "))
            }
        }
    }

    /// GpuFunction の呼び出し箇所の関数(生成コードをそのまま関数にする)。
    fn site_fn(&mut self, site: SiteId) -> String {
        if let Some(_) = self.site_fns.get(&site) {
            return format!("lg_site{site}");
        }
        let SiteExpansion::Gpu(g) = &self.a.expansions[site as usize] else {
            return format!("lg_site{site}");
        };
        let g = g.clone();
        for d in [&g.gen.defs] {
            if !d.trim().is_empty() && !self.defs.contains(d) {
                self.defs.push(d.clone());
            }
        }
        let returns: Vec<GpuTy> = g.returns.iter().map(GpuTy::from_spec).collect();
        let ret = match returns.len() {
            0 => "void".to_string(),
            1 => self.slang_type(&returns[0]),
            _ => {
                let mut def = format!("struct LgSRet{site} {{");
                for (i, t) in returns.iter().enumerate() {
                    let st = self.slang_type(t);
                    let _ = write!(def, " {st} r{i};");
                }
                def.push_str(" };\n");
                self.types.defs.push(def);
                format!("LgSRet{site}")
            }
        };
        let params: Vec<String> = g
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let st = self.slang_type(&GpuTy::from_spec(t));
                format!("{st} {}", crate::expand::slang_param_name(i))
            })
            .collect();
        let name = &self.a.program.site(site).name;
        let mut body = String::new();
        if !g.gen.stmts.trim().is_empty() {
            let _ = writeln!(body, "    {}", g.gen.stmts.trim());
        }
        match g.gen.exprs.len() {
            0 => {}
            1 => {
                let _ = writeln!(body, "    return {};", g.gen.exprs[0]);
            }
            _ => {
                let _ = writeln!(body, "    LgSRet{site} lg_r;");
                for (i, e) in g.gen.exprs.iter().enumerate() {
                    let _ = writeln!(body, "    lg_r.r{i} = {e};");
                }
                let _ = writeln!(body, "    return lg_r;");
            }
        }
        let src = format!(
            "// {name}\n{ret} lg_site{site}({}) {{\n{body}}}\n\n",
            params.join(", ")
        );
        self.site_fns.insert(site, src);
        format!("lg_site{site}")
    }

    // ---- 文 ---------------------------------------------------------------------

    fn on_gpu(&self, s: &Stmt) -> bool {
        self.in_func.is_some() || self.k.gpu_stmts.contains(&s.id)
    }

    fn block_stmts(&mut self, b: &Block) {
        for s in &b.stmts {
            if self.on_gpu(s) {
                self.stmt(s);
            }
        }
    }

    fn block(&mut self, b: &Block) {
        self.line("{");
        self.indent += 1;
        self.block_stmts(b);
        self.indent -= 1;
        self.line("}");
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Local { vars, exprs } => {
                let vals = self.values(exprs, vars.len());
                for (v, val) in vars.iter().zip(vals) {
                    self.declare(*v, val, s.span);
                }
            }
            StmtKind::Assign { targets, exprs } => {
                let vals = self.values(exprs, targets.len());
                // Lua はすべての右辺を評価してから代入する。
                let mut temps = Vec::new();
                for (t, val) in targets.iter().zip(vals) {
                    match val {
                        Some(code) if targets.len() > 1 => {
                            let ty = self.target_ty(t);
                            let st = self.slang_type(&ty);
                            let tmp = self.fresh("t");
                            self.line(&format!("{st} {tmp} = {code};"));
                            temps.push(Some(tmp));
                        }
                        other => temps.push(other),
                    }
                }
                for (t, val) in targets.iter().zip(temps) {
                    self.assign_target(t, val, s.span);
                }
            }
            StmtKind::Call(e) => {
                let code = self.call_code(e);
                if let Some(c) = code {
                    self.line(&format!("{c};"));
                }
            }
            StmtKind::Do(b) => self.block(b),
            StmtKind::If { arms, else_block } => {
                for (i, (cond, b)) in arms.iter().enumerate() {
                    let c = self.truthy(cond);
                    let kw = if i == 0 { "if" } else { "else if" };
                    self.line(&format!("{kw} ({c})"));
                    self.block(b);
                }
                if let Some(b) = else_block {
                    self.line("else");
                    self.block(b);
                }
            }
            StmtKind::While { cond, body } => {
                let g = self.fresh("g");
                let max = self.registry.options.max_loop_iterations;
                self.line(&format!("uint {g} = 0u;"));
                self.line("for (;;)");
                self.line("{");
                self.indent += 1;
                let c = self.truthy(cond);
                self.line(&format!("if (!({c})) break;"));
                self.line(&format!("if (++{g} > {max}u) break;"));
                self.loop_labels.push(end_label(body));
                self.block_stmts(body);
                self.loop_labels.pop();
                self.indent -= 1;
                self.line("}");
            }
            StmtKind::Repeat { body, cond } => {
                let g = self.fresh("g");
                let max = self.registry.options.max_loop_iterations;
                self.line(&format!("uint {g} = 0u;"));
                self.line("for (;;)");
                self.line("{");
                self.indent += 1;
                self.line(&format!("if (++{g} > {max}u) break;"));
                self.loop_labels.push(None);
                self.block_stmts(body);
                self.loop_labels.pop();
                let c = self.truthy(cond);
                self.line(&format!("if ({c}) break;"));
                self.indent -= 1;
                self.line("}");
            }
            StmtKind::NumFor {
                var,
                start,
                limit,
                step,
                body,
            } => {
                let st = self.fresh("s");
                let en = self.fresh("e");
                let sp = self.fresh("st");
                let ix = self.fresh("i");
                let g = self.fresh("g");
                let it = format!("lg_it{}", s.id);
                let max = self.registry.options.max_loop_iterations;
                let sc = self.scalar(start);
                let ec = self.scalar(limit);
                let pc = match step {
                    Some(e) => self.scalar(e),
                    None => "1.0".into(),
                };
                self.line("{");
                self.indent += 1;
                self.line(&format!("float {st} = {sc}; float {en} = {ec}; float {sp} = {pc};"));
                self.line(&format!("int {it} = 0; uint {g} = 0u;"));
                self.line(&format!(
                    "for (float {ix} = {st}; ({sp} > 0.0) ? ({ix} <= {en}) : ({ix} >= {en}); {ix} += {sp}, {it}++)"
                ));
                self.line("{");
                self.indent += 1;
                self.line(&format!("if (++{g} > {max}u) break;"));
                let vn = self.var_name(*var);
                self.line(&format!("float {vn} = {ix};"));
                self.loop_labels.push(end_label(body));
                self.block_stmts(body);
                self.loop_labels.pop();
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            StmtKind::Return(exprs) => {
                let f = self.in_func.unwrap_or(0);
                let sig = self.k.func_types.get(&f).cloned();
                let n = sig.as_ref().map(|s| s.returns.len()).unwrap_or(exprs.len());
                let vals: Vec<String> = self
                    .values(exprs, n)
                    .into_iter()
                    .zip(sig.map(|s| s.returns).unwrap_or_default())
                    .map(|(v, t)| v.unwrap_or_else(|| self.default_value(&t)))
                    .collect();
                let types = vec![GpuTy::Float; vals.len()];
                let r = self.make_return(&vals, &types, f);
                self.line(&r);
            }
            StmtKind::Break => self.line("break;"),
            StmtKind::Goto(label) => {
                if self.loop_labels.last().cloned().flatten().as_deref() == Some(label.as_str()) {
                    self.line("continue;");
                } else {
                    self.error(s.span, format!("`goto {label}` can only be used as `continue` (label at the end of the loop body) on the GPU"));
                }
            }
            StmtKind::Label(_) => {}
            StmtKind::LocalFunction { .. } | StmtKind::GenFor { .. } => {
                self.error(s.span, "this statement cannot run on the GPU");
            }
        }
    }

    fn target_ty(&self, t: &LValue) -> GpuTy {
        match t {
            LValue::Local(v, _) => self.k.var_types.get(v).cloned().unwrap_or(GpuTy::Float),
            LValue::Index { obj, .. } => match &obj.kind {
                ExprKind::Local(v) => match self.local_arrays.get(v) {
                    Some((elem, _)) => elem.clone(),
                    None => match self.k.var_types.get(v) {
                        Some(GpuTy::Record(_)) => GpuTy::Float,
                        _ => GpuTy::Float,
                    },
                },
                _ => GpuTy::Float,
            },
            _ => GpuTy::Float,
        }
    }

    fn declare(&mut self, v: VarId, val: Option<String>, span: Span) {
        let name = self.var_name(v);
        if let Some((elem, cap)) = self.local_arrays.get(&v).cloned() {
            let st = self.slang_type(&elem);
            let d = self.default_value(&elem);
            self.line(&format!("{st} {name}[{cap}];"));
            self.line(&format!("int {name}_n = 0;"));
            self.line(&format!("for (int lg_z = 0; lg_z < {cap}; lg_z++) {{ {name}[lg_z] = {d}; }}"));
            match val {
                Some(init) if init.starts_with("@array:") => {
                    let items: Vec<&str> = init["@array:".len()..].split('\u{1}').filter(|s| !s.is_empty()).collect();
                    for (i, item) in items.iter().enumerate() {
                        if (i as u32) < cap {
                            self.line(&format!("{name}[{i}] = {item};"));
                        }
                    }
                    self.line(&format!("{name}_n = {};", items.len().min(cap as usize)));
                }
                Some(_) => self.error(span, "arrays created on the GPU cannot be copied or aliased"),
                None => {}
            }
            return;
        }
        let ty = self.k.var_types.get(&v).cloned().unwrap_or(GpuTy::Float);
        let st = self.slang_type(&ty);
        let init = match val {
            Some(c) if c.starts_with("@array:") => {
                self.error(span, "this table cannot be represented on the GPU");
                self.default_value(&ty)
            }
            Some(c) => c,
            None => self.default_value(&ty),
        };
        self.line(&format!("{st} {name} = {init};"));
    }

    fn assign_target(&mut self, t: &LValue, val: Option<String>, span: Span) {
        let val = match val {
            Some(v) => v,
            None => {
                let ty = self.target_ty(t);
                self.default_value(&ty)
            }
        };
        match t {
            LValue::Local(v, _) => {
                if self.local_arrays.contains_key(v) {
                    self.error(span, "arrays created on the GPU cannot be reassigned");
                    return;
                }
                let n = self.var_name(*v);
                self.line(&format!("{n} = {val};"));
            }
            LValue::Index { obj, key, .. } => match &obj.kind {
                ExprKind::Local(v) if self.local_arrays.contains_key(v) => {
                    let (_, cap) = self.local_arrays[v].clone();
                    let n = self.var_name(*v);
                    let kc = self.scalar(key);
                    let idx = self.fresh("k");
                    self.line(&format!("int {idx} = lg_idx({kc});"));
                    self.line(&format!(
                        "if ({idx} >= 1 && {idx} <= {cap}) {{ {n}[{idx} - 1] = {val}; if ({idx} == {n}_n + 1) {{ {n}_n = {idx}; }} }}"
                    ));
                }
                ExprKind::Local(v) if matches!(self.k.var_types.get(v), Some(GpuTy::Record(_))) => {
                    match &key.kind {
                        ExprKind::Str(k) => {
                            let n = self.var_name(*v);
                            self.line(&format!("{n}.f_{} = {val};", sanitize(&String::from_utf8_lossy(k))));
                        }
                        _ => self.error(span, "record fields must be accessed with constant names on the GPU"),
                    }
                }
                _ => self.error(span, "only tables created in this loop can be modified on the GPU"),
            },
            LValue::Global(..) => self.error(span, "global variables cannot be assigned on the GPU"),
        }
    }

    /// 右辺の並びを n 個の値にする(多値の展開、足りない分は None)。
    fn values(&mut self, exprs: &[Expr], n: usize) -> Vec<Option<String>> {
        let mut out = Vec::new();
        for (i, e) in exprs.iter().enumerate() {
            let last = i + 1 == exprs.len();
            if last && n > exprs.len() && e.is_multi(&self.a.program) && !self.k.cpu_refs.contains_key(&ExprKey::of(e)) {
                let want = n - i;
                out.extend(self.multi(e, want).into_iter().map(Some));
            } else if let Some(arr) = self.array_ctor(e) {
                out.push(Some(arr));
            } else if i < n || !self.is_pure_value(e) {
                let c = self.value(e);
                if i < n {
                    out.push(c);
                } else if let Some(c) = c {
                    // 余った式も評価はする(GPU 関数の副作用のため)。
                    self.line(&format!("{c};"));
                }
            }
        }
        while out.len() < n {
            out.push(None);
        }
        out.truncate(n);
        out
    }

    fn is_pure_value(&self, e: &Expr) -> bool {
        !matches!(e.kind, ExprKind::Spec(_) | ExprKind::Call { .. } | ExprKind::Method { .. })
    }

    /// `{1, 2, 3}` → 配列初期化の印(declare で展開する)。
    fn array_ctor(&mut self, e: &Expr) -> Option<String> {
        if let ExprKind::Table(fields) = &e.kind {
            if fields.iter().all(|f| matches!(f, TableField::Positional(_))) && !fields.is_empty()
                || fields.is_empty()
            {
                let mut items = Vec::new();
                for f in fields {
                    if let TableField::Positional(v) = f {
                        items.push(self.scalar(v));
                    }
                }
                let mut s = String::from("@array:");
                for it in items {
                    s.push_str(&it);
                    s.push('\u{1}');
                }
                return Some(s);
            }
        }
        None
    }

    /// 値(関数呼び出しなら最初の戻り値)。値が無い(戻り値 0 個)なら None。
    fn value(&mut self, e: &Expr) -> Option<String> {
        if let ExprKind::Table(fields) = &e.kind {
            if fields.iter().all(|f| matches!(f, TableField::Named(..))) {
                let ty = self.expr_ty(e);
                let st = self.slang_type(&ty);
                let tmp = self.fresh("rec");
                self.line(&format!("{st} {tmp};"));
                for f in fields {
                    if let TableField::Named(k, v) = f {
                        let c = self.scalar_or_value(v);
                        self.line(&format!("{tmp}.f_{} = {c};", sanitize(k)));
                    }
                }
                return Some(tmp);
            }
        }
        if matches!(e.kind, ExprKind::Spec(_) | ExprKind::Call { .. } | ExprKind::Method { .. })
            && !self.k.cpu_refs.contains_key(&ExprKey::of(e))
        {
            return self.call_first(e);
        }
        Some(self.scalar(e))
    }

    fn scalar_or_value(&mut self, e: &Expr) -> String {
        self.value(e).unwrap_or_else(|| "0.0".into())
    }

    /// 関数呼び出しの最初の戻り値。
    fn call_first(&mut self, e: &Expr) -> Option<String> {
        let n = self.call_return_count(e);
        if n == 0 {
            let c = self.call_code(e)?;
            self.line(&format!("{c};"));
            return None;
        }
        if n == 1 {
            return self.call_code(e);
        }
        let v = self.multi(e, 1);
        v.into_iter().next()
    }

    fn call_return_count(&self, e: &Expr) -> usize {
        match &e.kind {
            ExprKind::Spec(site) => match self.a.targets[*site as usize] {
                SiteTarget::Gpu => match &self.a.expansions[*site as usize] {
                    SiteExpansion::Gpu(g) => g.returns.len(),
                    _ => 1,
                },
                _ => self.func_of(e).and_then(|f| self.k.func_types.get(&f)).map(|s| s.returns.len()).unwrap_or(1),
            },
            _ => match self.resolver.callee(e) {
                Some(StaticValue::Func(f)) => self.k.func_types.get(&f).map(|s| s.returns.len()).unwrap_or(1),
                Some(StaticValue::Builtin(b)) => match b.name {
                    "math.modf" | "math.frexp" => 2,
                    "error" => 0,
                    _ => 1,
                },
                _ => 1,
            },
        }
    }

    fn func_of(&self, e: &Expr) -> Option<FuncId> {
        match &e.kind {
            ExprKind::Spec(site) => match &self.a.expansions[*site as usize] {
                SiteExpansion::Cpu(c) if self.a.targets[*site as usize] == SiteTarget::Cpu => Some(c.func),
                _ => None,
            },
            _ => match self.resolver.callee(e) {
                Some(StaticValue::Func(f)) => Some(f),
                _ => None,
            },
        }
    }

    /// 多値の呼び出しを一時変数に受けて、n 個の値を返す。
    fn multi(&mut self, e: &Expr, n: usize) -> Vec<String> {
        let count = self.call_return_count(e);
        let Some(code) = self.call_code(e) else {
            return vec!["0.0".into(); n];
        };
        if count <= 1 {
            let mut v = vec![code];
            while v.len() < n {
                v.push("0.0".into());
            }
            return v;
        }
        let ty = match &e.kind {
            ExprKind::Spec(site) if self.a.targets[*site as usize] == SiteTarget::Gpu => format!("LgSRet{site}"),
            _ => match self.func_of(e) {
                Some(f) => format!("LgRet{f}"),
                None => "LgPair".into(),
            },
        };
        let tmp = self.fresh("m");
        self.line(&format!("{ty} {tmp} = {code};"));
        (0..n)
            .map(|i| if i < count { format!("{tmp}.r{i}") } else { "0.0".into() })
            .collect()
    }

    // ---- 式 ---------------------------------------------------------------------

    fn truthy(&mut self, e: &Expr) -> String {
        let ty = self.expr_ty(e);
        let c = self.scalar(e);
        match ty {
            GpuTy::Bool => c,
            // Lua では nil と false 以外はすべて真(数値の 0 も真)。
            _ => "true".into(),
        }
    }

    /// 単一の値の式(スカラーまたは CPU から渡した構造体)。
    fn scalar(&mut self, e: &Expr) -> String {
        if let Some(r) = self.k.cpu_refs.get(&ExprKey::of(e)).copied() {
            if self.in_func.is_none() {
                return self.cpu_ref(r);
            }
        }
        match &e.kind {
            ExprKind::Number(n) => float_lit(*n),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Paren(inner) => self.scalar(inner),
            ExprKind::Local(v) => {
                if self.local_arrays.contains_key(v) {
                    self.error(e.span, "arrays created on the GPU can only be indexed");
                    return "0.0".into();
                }
                let r = VarRef::Local(*v);
                if self.k.carried.contains(&r) {
                    self.error(e.span, "internal: carried variable read on the GPU without a record");
                }
                // 捕捉した外側の local は static、それ以外(本体・関数の local)はそのままの名前。
                if let Some(name) = self.cap_names.get(&format!("local:{v}")) {
                    return name.clone();
                }
                self.var_name(*v)
            }
            ExprKind::Global(g) => match self.cap_names.get(&format!("global:{g}")) {
                Some(n) => n.clone(),
                None => {
                    self.error(e.span, format!("global `{g}` is not available on the GPU"));
                    "0.0".into()
                }
            },
            ExprKind::Index { obj, key } => self.index(e, obj, key),
            ExprKind::Binary { op, lhs, rhs } => self.binary(e, *op, lhs, rhs),
            ExprKind::Unary { op, expr } => match op {
                UnOp::Neg => format!("(-{})", self.scalar(expr)),
                UnOp::Not => {
                    let t = self.expr_ty(expr);
                    let c = self.scalar(expr);
                    match t {
                        GpuTy::Bool => format!("(!{c})"),
                        _ => "false".into(),
                    }
                }
                UnOp::Len => match &expr.kind {
                    ExprKind::Local(v) if self.local_arrays.contains_key(v) => {
                        format!("float({}_n)", self.var_name(*v))
                    }
                    _ => {
                        let c = self.scalar(expr);
                        format!("float({c}.len)")
                    }
                },
            },
            ExprKind::Spec(_) | ExprKind::Call { .. } | ExprKind::Method { .. } => {
                self.call_first(e).unwrap_or_else(|| "0.0".into())
            }
            ExprKind::Table(_) => self.value(e).unwrap_or_else(|| "0.0".into()),
            _ => {
                self.error(e.span, "this expression cannot be evaluated on the GPU");
                "0.0".into()
            }
        }
    }

    fn cpu_ref(&mut self, r: CpuRef) -> String {
        match r {
            CpuRef::Hoist(i) => format!("lg_h{i}"),
            CpuRef::Record(i) => {
                let rec = &self.k.records[i];
                let load = if rec.ty == GpuTy::Bool { "lg_b" } else { "lg_f" };
                let slots = self.k.slots_per_iteration;
                let at = match rec.record_loop {
                    Some(l) => {
                        let lp = &self.k.record_loops[l];
                        format!(
                            "lg_rec_base + lg_iter * {slots}u + {}u + uint(lg_it{}) * {}u + {}u",
                            lp.base, lp.stmt, lp.stride, rec.offset
                        )
                    }
                    None => format!("lg_rec_base + lg_iter * {slots}u + {}u", rec.offset),
                };
                format!("{load}({at})")
            }
        }
    }

    fn index(&mut self, e: &Expr, obj: &Expr, key: &Expr) -> String {
        if let Some(name) = crate::kernel::classify::external_name(e) {
            return crate::expand::slang_external_name(name);
        }
        if let Some(StaticValue::Builtin(b)) = self.resolver.value_of(e) {
            if let GpuImpl::Const(v) = b.gpu {
                return float_lit(v);
            }
        }
        if let ExprKind::Local(v) = obj.kind {
            if let Some((elem, cap)) = self.local_arrays.get(&v).cloned() {
                let n = self.var_name(v);
                let kc = self.scalar(key);
                let idx = self.fresh("k");
                self.line(&format!("int {idx} = lg_idx({kc});"));
                let d = self.default_value(&elem);
                let _ = cap;
                return format!("(({idx} >= 1 && {idx} <= {n}_n) ? {n}[{idx} - 1] : {d})");
            }
        }
        let oc = self.scalar(obj);
        match (&self.expr_ty(obj), &key.kind) {
            (GpuTy::Record(_), ExprKind::Str(k)) => {
                format!("{oc}.f_{}", sanitize(&String::from_utf8_lossy(k)))
            }
            _ => {
                let kc = self.scalar(key);
                format!("{oc}[lg_idx({kc}) - 1]")
            }
        }
    }

    fn binary(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            if let Some(folded) = self.fold_type_compare(lhs, rhs) {
                let r = if op == BinOp::Eq { folded } else { !folded };
                return r.to_string();
            }
        }
        // `c and a or b` → (c と a が真) ? a : b。
        if op == BinOp::Or {
            if let ExprKind::Binary {
                op: BinOp::And,
                lhs: c,
                rhs: a,
            } = &lhs.kind
            {
                let cc = self.truthy(c);
                let at = self.expr_ty(a);
                let ac = self.scalar(a);
                let bc = self.scalar(rhs);
                let a_truthy = if at == GpuTy::Bool { ac.clone() } else { "true".into() };
                return format!("(({cc} && {a_truthy}) ? {ac} : {bc})");
            }
        }
        let lt = self.expr_ty(lhs);
        let rt = self.expr_ty(rhs);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let l = self.scalar(lhs);
                let r = self.scalar(rhs);
                format!("({l} {} {r})", op.lua_symbol())
            }
            BinOp::Mod => format!("lg_mod({}, {})", self.scalar(lhs), self.scalar(rhs)),
            BinOp::Pow => format!("lg_pow({}, {})", self.scalar(lhs), self.scalar(rhs)),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l = self.scalar(lhs);
                let r = self.scalar(rhs);
                format!("({l} {} {r})", op.lua_symbol())
            }
            BinOp::Eq | BinOp::Ne => {
                let l = self.scalar(lhs);
                let r = self.scalar(rhs);
                if lt.is_scalar() && rt.is_scalar() && lt != rt {
                    // 型が違う値は等しくない。
                    return (op == BinOp::Ne).to_string();
                }
                let sym = if op == BinOp::Eq { "==" } else { "!=" };
                format!("({l} {sym} {r})")
            }
            BinOp::And => {
                let l = self.scalar(lhs);
                let r = self.scalar(rhs);
                match lt {
                    GpuTy::Bool => format!("({l} && {r})"),
                    // 数値は常に真なので `a and b` は b。
                    _ => r,
                }
            }
            BinOp::Or => {
                let l = self.scalar(lhs);
                let r = self.scalar(rhs);
                match lt {
                    GpuTy::Bool => format!("({l} || {r})"),
                    _ => l,
                }
            }
            BinOp::Concat => {
                self.error(e.span, "string concatenation cannot run on the GPU");
                "0.0".into()
            }
        }
    }

    /// `type(x) == "number"` を静的な型で畳み込む。
    fn fold_type_compare(&mut self, lhs: &Expr, rhs: &Expr) -> Option<bool> {
        let (call, lit) = match (&lhs.kind, &rhs.kind) {
            (_, ExprKind::Str(s)) => (lhs, s),
            (ExprKind::Str(s), _) => (rhs, s),
            _ => return None,
        };
        let Some(StaticValue::Builtin(b)) = self.resolver.callee(call) else {
            return None;
        };
        if b.name != "type" {
            return None;
        }
        let arg = match &call.kind {
            ExprKind::Call { args, .. } => args.first()?,
            _ => return None,
        };
        let name = match self.expr_ty(arg) {
            GpuTy::Float => "number",
            GpuTy::Bool => "boolean",
            GpuTy::Array(..) | GpuTy::Record(_) => "table",
            GpuTy::Func(_) | GpuTy::Builtin(_) => "function",
            _ => return None,
        };
        Some(lit.as_slice() == name.as_bytes())
    }

    /// 呼び出し式のコード(戻り値の型はそのまま: 単一値 / 構造体 / void)。
    fn call_code(&mut self, e: &Expr) -> Option<String> {
        if let Some(r) = self.k.cpu_refs.get(&ExprKey::of(e)).copied() {
            if self.in_func.is_none() {
                return Some(self.cpu_ref(r));
            }
        }
        match &e.kind {
            ExprKind::Spec(site) => {
                let args = self.a.program.site(*site).args.clone();
                match self.a.targets[*site as usize] {
                    SiteTarget::Gpu => {
                        let name = self.site_fn(*site);
                        let a: Vec<String> = args.iter().map(|x| self.scalar_or_value(x)).collect();
                        Some(format!("{name}({})", a.join(", ")))
                    }
                    _ => match self.func_of(e) {
                        Some(f) => {
                            let a = self.call_args(&args);
                            Some(format!("lg_fn{f}({})", a.join(", ")))
                        }
                        None => {
                            self.error(e.span, "this function cannot be called on the GPU");
                            None
                        }
                    },
                }
            }
            ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => match self.resolver.callee(e) {
                Some(StaticValue::Func(f)) => {
                    let a = self.call_args(args);
                    Some(format!("lg_fn{f}({})", a.join(", ")))
                }
                Some(StaticValue::Builtin(b)) => self.builtin(e, b, args),
                _ => {
                    self.error(e.span, "this function cannot be called on the GPU");
                    None
                }
            },
            _ => Some(self.scalar(e)),
        }
    }

    fn call_args(&mut self, args: &[Expr]) -> Vec<String> {
        let mut out = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if i + 1 == args.len() && a.is_multi(&self.a.program) && !self.k.cpu_refs.contains_key(&ExprKey::of(a)) {
                let n = self.call_return_count(a);
                out.extend(self.multi(a, n.max(1)));
            } else {
                out.push(self.scalar_or_value(a));
            }
        }
        out
    }

    fn builtin(&mut self, e: &Expr, b: &'static Builtin, args: &[Expr]) -> Option<String> {
        let vals: Vec<String> = match b.name {
            "string.len" | "string.byte" => Vec::new(),
            _ => args.iter().map(|a| self.scalar_or_value(a)).collect(),
        };
        let arg = |i: usize| vals.get(i).cloned().unwrap_or_else(|| "0.0".into());
        Some(match (b.name, b.gpu) {
            ("math.max", _) | ("math.min", _) => {
                let h = if b.name == "math.max" { "lg_max" } else { "lg_min" };
                let mut acc = arg(0);
                for v in vals.iter().skip(1) {
                    acc = format!("{h}({acc}, {v})");
                }
                acc
            }
            ("bit.band", _) | ("bit.bor", _) | ("bit.bxor", _) => {
                let h = format!("lg_{}", &b.name[4..]);
                let mut acc = format!("lg_tobit({})", arg(0));
                for v in vals.iter().skip(1) {
                    acc = format!("{h}({acc}, {v})");
                }
                acc
            }
            ("math.log", _) => {
                if vals.len() >= 2 {
                    format!("lg_log2arg({}, {})", arg(0), arg(1))
                } else {
                    format!("lg_log({})", arg(0))
                }
            }
            (_, GpuImpl::Intrinsic(name)) => format!("{name}({})", vals.join(", ")),
            (_, GpuImpl::Helper(name)) => format!("{name}({})", vals.join(", ")),
            ("tonumber", _) | ("assert", _) => arg(0),
            ("rawequal", _) => format!("({} == {})", arg(0), arg(1)),
            ("rawget", _) => format!("{}[lg_idx({}) - 1]", arg(0), arg(1)),
            ("error", _) => return None,
            ("string.len", _) => match args.first().map(|a| &a.kind) {
                Some(ExprKind::Str(s)) => float_lit(s.len() as f64),
                _ => {
                    self.error(e.span, "string.len needs a constant string on the GPU");
                    "0.0".into()
                }
            },
            ("string.byte", _) => match args.first().map(|a| &a.kind) {
                Some(ExprKind::Str(s)) => {
                    let name = format!("lg_str{}", self.str_consts.len());
                    let items: Vec<String> = s.iter().map(|c| format!("{c}.0")).collect();
                    let len = s.len().max(1);
                    let items = if items.is_empty() { vec!["0.0".to_string()] } else { items };
                    self.str_consts.push(format!(
                        "static const float {name}[{len}] = {{ {} }};\n",
                        items.join(", ")
                    ));
                    let i = match args.get(1) {
                        Some(a) => self.scalar(a),
                        None => "1.0".into(),
                    };
                    let k = self.fresh("k");
                    self.line(&format!("int {k} = lg_idx({i});"));
                    format!("(({k} >= 1 && {k} <= {}) ? {name}[{k} - 1] : 0.0)", s.len())
                }
                _ => {
                    self.error(e.span, "string.byte needs a constant string on the GPU");
                    "0.0".into()
                }
            },
            _ => {
                self.error(e.span, format!("`{}` cannot be used on the GPU", b.name));
                "0.0".into()
            }
        })
    }
}

/// ブロックの末尾のラベル(`goto continue` を continue に変換するため)。
fn end_label(b: &Block) -> Option<String> {
    match b.stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Label(l)) => Some(l.clone()),
        _ => None,
    }
}

fn ends_with_return(b: &Block) -> bool {
    matches!(b.stmts.last().map(|s| &s.kind), Some(StmtKind::Return(_)))
}
