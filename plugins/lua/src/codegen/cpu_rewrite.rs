//! CPU で実行する Lua の生成。
//!
//! 元のソースのうち、次の範囲だけを書き換える(それ以外はコメントや空白も含めて元のまま):
//! - カーネルにした文 → ディスパッチ用ブロック(1 行。元の行数ぶん改行を補って行番号を保つ)
//! - CpuFunction の呼び出し → `__lg_cf[<site>](...)`(引数の部分は元のまま)
//! - カーネルの外の GpuFunction の呼び出し → `__lg_gpu_only("name")`(実行時エラー)
//! - 定数パスの dofile → 同梱したチャンクの呼び出し `__lg_df[<n>]()`
//!
//! 1 行目にレイアウトのヘッダーと、生成関数・同梱モジュールの定義を置き、そのあとに
//! メインのソースが(1 行目から)続く。

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::analysis::resolve::Resolver;
use crate::codegen::layout::{encode_header, KernelLayout};
use crate::codegen::lua_print::{lua_string, LuaPrinter};
use crate::codegen::slang::{Entry, SlangKernel};
use crate::diag::Diagnostics;
use crate::expand::{SiteExpansion, SiteTarget};
use crate::hir::*;
use crate::kernel::bt::{Bt, BtCx};
use crate::kernel::classify::free_locals;
use crate::kernel::{CaptureSource, CpuRef, ExprKey, Kernel, VarRef};
use crate::pipeline::Analysis;

pub fn generate(
    a: &Analysis,
    kernels: &[(&Kernel, &SlangKernel)],
    diags: &mut Diagnostics,
) -> Option<String> {
    let resolver = Resolver::new(&a.program);
    let gpu_funcs = crate::kernel::detect::gpu_funcs(&a.program, &resolver, &a.targets);
    let mut w = Writer {
        a,
        resolver: &resolver,
        gpu_funcs: &gpu_funcs,
        diags,
        errors: false,
    };
    w.run(kernels)
}

struct Writer<'a> {
    a: &'a Analysis,
    resolver: &'a Resolver<'a>,
    gpu_funcs: &'a HashSet<FuncId>,
    diags: &'a mut Diagnostics,
    errors: bool,
}

fn newline_count(s: &str) -> usize {
    s.bytes().filter(|b| *b == b'\n').count()
}

impl<'a> Writer<'a> {
    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.errors = true;
        let loc = self.a.program.location(span);
        self.diags.error("cpu-codegen", loc, msg);
    }

    fn run(&mut self, kernels: &[(&Kernel, &SlangKernel)]) -> Option<String> {
        let layouts: Vec<KernelLayout> = kernels.iter().map(|(_, s)| s.layout.clone()).collect();
        let chunk = self.a.program.files[self.a.program.main_file as usize].chunk.clone();
        let mut out = encode_header(&chunk, &layouts);
        out.push_str(" __lg_cf = {}; __lg_df = {};");

        // 生成関数(CpuFunction)。
        for (i, e) in self.a.expansions.iter().enumerate() {
            if let SiteExpansion::Cpu(c) = e {
                if self.a.targets[i] != SiteTarget::Cpu {
                    continue;
                }
                let site = self.a.program.site(i as SiteId);
                let loc = self.a.program.location(site.span);
                let name = format!("={}@{}:{}", site.name, loc.chunk, loc.line);
                let _ = write!(
                    out,
                    " __lg_cf[{i}] = __lg_load({}, {})();",
                    lua_string(format!("return {}", c.source).as_bytes()),
                    lua_string(name.as_bytes())
                );
            }
        }

        // 同梱するモジュール / dofile のファイル。
        let files: Vec<SourceFile> = self.a.program.files.clone();
        for (name, fid) in self.a.program.modules.clone() {
            let src = self.rewrite_file(fid, kernels);
            let chunk = chunk_name(&files[fid as usize]);
            let _ = write!(
                out,
                " __lg_pre[{}] = __lg_load({}, {});",
                lua_string(name.as_bytes()),
                lua_string(src.as_bytes()),
                lua_string(chunk.as_bytes())
            );
        }
        for (idx, (_, fid)) in self.a.program.dofiles.clone().into_iter().enumerate() {
            let src = self.rewrite_file(fid, kernels);
            let chunk = chunk_name(&files[fid as usize]);
            let _ = write!(
                out,
                " __lg_df[{}] = __lg_load({}, {});",
                idx + 1,
                lua_string(src.as_bytes()),
                lua_string(chunk.as_bytes())
            );
        }

        // メイン(1 行目の続きから)。
        let main = self.rewrite_file(self.a.program.main_file, kernels);
        out.push(' ');
        if main.starts_with('#') {
            // シバン行はチャンクの先頭でしか使えないので、コメントにする。
            out.push_str("--");
        }
        out.push_str(&main);
        if self.errors {
            None
        } else {
            Some(out)
        }
    }

    /// ファイルのソースを書き換える。
    fn rewrite_file(&mut self, file: FileId, kernels: &[(&Kernel, &SlangKernel)]) -> String {
        let src = self.a.program.files[file as usize].source.clone();
        let mut edits: Vec<(u32, u32, String)> = Vec::new();

        let kernel_ranges: Vec<(u32, u32)> = kernels
            .iter()
            .filter(|(k, _)| k.span.file == file)
            .map(|(k, _)| (k.span.start, k.span.end))
            .collect();
        let inside_kernel = |s: u32, e: u32| kernel_ranges.iter().any(|(ks, ke)| s >= *ks && e <= *ke);

        for (k, sk) in kernels.iter().filter(|(k, _)| k.span.file == file) {
            let mut text = self.dispatch_block(k, sk);
            let original = &src[k.span.start as usize..k.span.end as usize];
            text.push_str(&"\n".repeat(newline_count(original)));
            edits.push((k.span.start, k.span.end, text));
        }

        let mut covering: Vec<(u32, u32)> = Vec::new();
        for site in &self.a.program.sites {
            if site.span.file != file || inside_kernel(site.chain_start, site.span.end) {
                continue;
            }
            match (self.a.targets[site.id as usize], &self.a.expansions[site.id as usize]) {
                (SiteTarget::Cpu, SiteExpansion::Cpu(_)) => {
                    let text = if site.property {
                        format!("__lg_cf[{}]()", site.id)
                    } else {
                        format!("__lg_cf[{}]", site.id)
                    };
                    edits.push((site.chain_start, site.callee_end, text));
                }
                (SiteTarget::Gpu, _) => {
                    let original = &src[site.chain_start as usize..site.span.end as usize];
                    let mut text = format!("__lg_gpu_only({})", lua_string(site.name.as_bytes()));
                    text.push_str(&"\n".repeat(newline_count(original)));
                    edits.push((site.chain_start, site.span.end, text));
                    covering.push((site.chain_start, site.span.end));
                }
                _ => {}
            }
        }
        for call in &self.a.program.dofile_calls {
            if call.span.file != file || inside_kernel(call.span.start, call.span.end) {
                continue;
            }
            if let Some(idx) = self.a.program.dofiles.get_index_of(&call.path) {
                let original = &src[call.span.start as usize..call.span.end as usize];
                let mut text = format!("__lg_df[{}]()", idx + 1);
                text.push_str(&"\n".repeat(newline_count(original)));
                edits.push((call.span.start, call.span.end, text));
                covering.push((call.span.start, call.span.end));
            }
        }

        // 別の置き換えに含まれる置き換えは捨てる(GPU 関数の呼び出しの引数の中など)。
        edits.sort_by_key(|(s, e, _)| (*s, std::cmp::Reverse(*e)));
        let mut out = String::with_capacity(src.len());
        let mut pos = 0u32;
        for (s, e, text) in edits {
            if s < pos {
                continue;
            }
            out.push_str(&src[pos as usize..s as usize]);
            out.push_str(&text);
            pos = e;
        }
        out.push_str(&src[pos as usize..]);
        let _ = covering;
        out
    }

    // ---- ディスパッチ用ブロック ---------------------------------------------------

    fn dispatch_block(&mut self, k: &Kernel, sk: &SlangKernel) -> String {
        let mut p = LuaPrinter::new(self.a);
        for (i, h) in k.hoists.iter().enumerate() {
            p.hoists.insert(ExprKey::of(&h.expr), format!("__lg_h{i}"));
        }
        let slots = k.slots_per_iteration;
        for r in &k.records {
            let slot = match r.record_loop {
                Some(l) => format!("__lg_bl{} + {}", k.record_loops[l].stmt, r.offset),
                None => format!("__lg_b + {}", r.offset),
            };
            p.records.insert(ExprKey::of(&r.expr), slot);
        }

        let mut s = String::from("do ");
        // 次元の上下限(元の for と同じく 1 回だけ評価する)。
        for (d, dim) in k.dims.iter().enumerate() {
            let step = dim.step.as_ref().map(|e| p.expr(e)).unwrap_or_else(|| "1".into());
            let _ = write!(
                s,
                "local __lg_s{d}, __lg_e{d}, __lg_t{d} = {}, {}, {step}; local __lg_n{d} = __lg_count(__lg_s{d}, __lg_e{d}, __lg_t{d}); ",
                p.expr(&dim.start),
                p.expr(&dim.limit)
            );
        }
        let total = if k.dims.is_empty() {
            "1".to_string()
        } else {
            (0..k.dims.len()).map(|d| format!("__lg_n{d}")).collect::<Vec<_>>().join(" * ")
        };
        let _ = write!(s, "local __lg_n = {total}; local __lg_rec = {{}}; ");

        // 持ち出し(反復が 1 回もなければ評価しない)。
        if !k.hoists.is_empty() {
            let names: Vec<String> = (0..k.hoists.len()).map(|i| format!("__lg_h{i}")).collect();
            let _ = write!(s, "local {}; if __lg_n > 0 then ", names.join(", "));
            let raw = LuaPrinter::new(self.a);
            for (i, h) in k.hoists.iter().enumerate() {
                let _ = write!(s, "__lg_h{i} = {}; ", raw.expr(&h.expr));
            }
            s.push_str("end ");
        }

        // 再生。
        if k.needs_replay {
            let mut body = String::new();
            let cx = self.replay_cx(k);
            self.replay_block(&p, &cx, k, &k.body, false, &mut body);
            if k.dims.is_empty() {
                let _ = write!(s, "do local __lg_b = 1; {body}end ");
            } else {
                s.push_str("local __lg_it = 0; ");
                for (d, dim) in k.dims.iter().enumerate() {
                    let _ = write!(
                        s,
                        "for {} = __lg_s{d}, __lg_e{d}, __lg_t{d} do ",
                        p.var(dim.var)
                    );
                }
                let _ = write!(s, "local __lg_b = __lg_it * {slots} + 1; __lg_it = __lg_it + 1; {body}");
                for _ in &k.dims {
                    s.push_str("end ");
                }
            }
        }

        // params の値。
        let mut values = Vec::new();
        for e in &sk.entries {
            values.push(match e {
                Entry::Total => "__lg_n".to_string(),
                Entry::DimStart(d) => format!("__lg_s{d}"),
                Entry::DimStep(d) => format!("__lg_t{d}"),
                Entry::DimCount(d) => format!("__lg_n{d}"),
                Entry::Hoist(i) => format!("__lg_h{i}"),
                Entry::Records => "__lg_rec".to_string(),
                Entry::Capture(src) => self.capture_value(k, src),
            });
        }
        let _ = write!(s, "__lg_dispatch({}, {{{}}}) end", k.id, values.join(", "));
        s
    }

    fn capture_value(&mut self, k: &Kernel, src: &CaptureSource) -> String {
        match src {
            CaptureSource::External(n) => format!("__lg_ext.{n}"),
            CaptureSource::Var(VarRef::Global(g)) => g.clone(),
            CaptureSource::Var(VarRef::Local(v)) => {
                let info = self.a.program.var(*v);
                // ディスパッチ用ブロックの位置から見える local でなければならない。
                let mut f = Some(k.func);
                let mut visible = false;
                while let Some(cur) = f {
                    if cur == info.func {
                        visible = true;
                        break;
                    }
                    f = self.a.program.func(cur).parent;
                }
                if !visible || (info.span.file == k.span.file && info.span.start > k.span.start) {
                    let name = info.name.clone();
                    self.error(
                        k.span,
                        format!("`{name}` is used on the GPU (through a function) but is not visible where the loop is"),
                    );
                }
                info.name.clone()
            }
        }
    }

    // ---- 再生 ---------------------------------------------------------------------

    fn replay_cx(&self, k: &Kernel) -> ReplayCx<'a> {
        let dim_vars = k.dims.iter().map(|d| d.var).collect();
        let mut bt = BtCx::new(&self.a.program, self.resolver, &self.a.targets, dim_vars, self.gpu_funcs);
        bt.result.var_bt = k.var_bt.clone();
        ReplayCx {
            bt,
            leaves: k.replay_leaves.clone(),
            controls: k.replay_controls.clone(),
            record_stmts: k.record_stmts.clone(),
            record_loops: k
                .record_loops
                .iter()
                .map(|l| (l.stmt, (l.base, l.stride, l.max_iterations)))
                .collect(),
        }
    }

    fn contains(&self, cx: &ReplayCx, s: &Stmt) -> bool {
        if cx.leaves.contains(&s.id) || cx.controls.contains(&s.id) || cx.record_stmts.contains(&s.id) {
            return true;
        }
        let any = |b: &Block| b.stmts.iter().any(|x| self.contains(cx, x));
        match &s.kind {
            StmtKind::Do(b) => any(b),
            StmtKind::If { arms, else_block } => {
                arms.iter().any(|(_, b)| any(b)) || else_block.as_ref().is_some_and(any)
            }
            StmtKind::While { body, .. }
            | StmtKind::Repeat { body, .. }
            | StmtKind::NumFor { body, .. }
            | StmtKind::GenFor { body, .. } => any(body),
            _ => false,
        }
    }

    fn replay_block(&self, p: &LuaPrinter, cx: &ReplayCx, k: &Kernel, b: &Block, flatten: bool, out: &mut String) {
        for s in &b.stmts {
            self.replay_stmt(p, cx, k, s, flatten, out);
        }
    }

    /// 文に含まれる記録の式を評価順に出力する(その文自体は再生しない場合)。
    fn standalone_records(&self, p: &LuaPrinter, k: &Kernel, s: &Stmt, out: &mut String) {
        let mut exprs: Vec<Expr> = Vec::new();
        let mut visit = |e: &Expr| collect_records(&self.a.program, e, k, &mut exprs);
        match &s.kind {
            StmtKind::Local { exprs: es, .. } | StmtKind::Return(es) => es.iter().for_each(&mut visit),
            StmtKind::Assign { targets, exprs: es } => {
                es.iter().for_each(&mut visit);
                for t in targets {
                    if let LValue::Index { obj, key, .. } = t {
                        visit(obj);
                        visit(key);
                    }
                }
            }
            StmtKind::Call(e) => visit(e),
            StmtKind::If { arms, .. } => arms.iter().for_each(|(c, _)| visit(c)),
            StmtKind::While { cond, .. } | StmtKind::Repeat { cond, .. } => visit(cond),
            StmtKind::NumFor { start, limit, step, .. } => {
                visit(start);
                visit(limit);
                if let Some(st) = step {
                    visit(st);
                }
            }
            _ => {}
        }
        for e in &exprs {
            let _ = write!(out, "{}; ", p.expr(e));
        }
    }

    fn replay_stmt(&self, p: &LuaPrinter, cx: &ReplayCx, k: &Kernel, s: &Stmt, flatten: bool, out: &mut String) {
        if cx.leaves.contains(&s.id) {
            p.stmt(s, out);
            return;
        }
        match &s.kind {
            StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) => {
                if !flatten {
                    p.stmt(s, out);
                }
                return;
            }
            _ => {}
        }
        if !self.contains(cx, s) {
            return;
        }
        match &s.kind {
            StmtKind::Local { .. } | StmtKind::Assign { .. } | StmtKind::Call(_) | StmtKind::Return(_) => {
                self.standalone_records(p, k, s, out);
            }
            StmtKind::Do(b) => {
                out.push_str("do ");
                self.replay_block(p, cx, k, b, flatten, out);
                out.push_str("end ");
            }
            StmtKind::If { arms, else_block } => {
                let cond_p = arms.iter().any(|(c, _)| cx.bt.expr(c) == Bt::P);
                if flatten || cond_p {
                    // GPU の値で分岐する: 中の(純粋な)記録だけを無条件に評価する。
                    self.standalone_records(p, k, s, out);
                    for (_, b) in arms {
                        self.replay_block(p, cx, k, b, true, out);
                    }
                    if let Some(b) = else_block {
                        self.replay_block(p, cx, k, b, true, out);
                    }
                } else {
                    for (i, (c, b)) in arms.iter().enumerate() {
                        let kw = if i == 0 { "if" } else { "elseif" };
                        let _ = write!(out, "{kw} {} then ", p.expr(c));
                        self.replay_block(p, cx, k, b, false, out);
                    }
                    if let Some(b) = else_block {
                        out.push_str("else ");
                        self.replay_block(p, cx, k, b, false, out);
                    }
                    out.push_str("end ");
                }
            }
            StmtKind::While { cond, body } => {
                if flatten || cx.bt.expr(cond) == Bt::P {
                    return;
                }
                let _ = write!(out, "while {} do ", p.expr(cond));
                self.replay_block(p, cx, k, body, false, out);
                out.push_str("end ");
            }
            StmtKind::Repeat { body, cond } => {
                if flatten || cx.bt.expr(cond) == Bt::P {
                    return;
                }
                out.push_str("repeat ");
                self.replay_block(p, cx, k, body, false, out);
                let _ = write!(out, "until {} ", p.expr(cond));
            }
            StmtKind::NumFor { var, start, limit, step, body } => {
                let bounds_p = [Some(start), Some(limit), step.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|e| cx.bt.expr(e) == Bt::P);
                if flatten || bounds_p {
                    return;
                }
                let step_s = step.as_ref().map(|e| format!(", {}", p.expr(e))).unwrap_or_default();
                match cx.record_loops.get(&s.id) {
                    Some((base, stride, max)) => {
                        let id = s.id;
                        let _ = write!(
                            out,
                            "local __lg_it{id} = 0; for {} = {}, {}{step_s} do local __lg_bl{id} = __lg_b + {base} + __lg_it{id} * {stride}; __lg_it{id} = __lg_it{id} + 1; if __lg_it{id} > {max} then __lg_overflow({max}) end ",
                            p.var(*var),
                            p.expr(start),
                            p.expr(limit)
                        );
                    }
                    None => {
                        let _ = write!(out, "for {} = {}, {}{step_s} do ", p.var(*var), p.expr(start), p.expr(limit));
                    }
                }
                self.replay_block(p, cx, k, body, false, out);
                out.push_str("end ");
            }
            StmtKind::GenFor { .. } | StmtKind::LocalFunction { .. } => p.stmt(s, out),
            StmtKind::Break | StmtKind::Goto(_) | StmtKind::Label(_) => {}
        }
    }
}

struct ReplayCx<'a> {
    bt: BtCx<'a>,
    leaves: HashSet<StmtId>,
    controls: HashSet<StmtId>,
    record_stmts: HashSet<StmtId>,
    record_loops: HashMap<StmtId, (u32, u32, u32)>,
}

/// 式の中の記録の式を評価順に集める(記録の中には入らない)。
fn collect_records(program: &Program, e: &Expr, k: &Kernel, out: &mut Vec<Expr>) {
    if matches!(k.cpu_refs.get(&ExprKey::of(e)), Some(CpuRef::Record(_))) {
        out.push(e.clone());
        return;
    }
    // 子の式を順に見る(spec の引数も)。
    match &e.kind {
        ExprKind::Spec(site) => {
            for a in &program.site(*site).args {
                collect_records(program, a, k, out);
            }
        }
        ExprKind::Index { obj, key } => {
            collect_records(program, obj, k, out);
            collect_records(program, key, k, out);
        }
        ExprKind::Call { func, args } => {
            collect_records(program, func, k, out);
            for a in args {
                collect_records(program, a, k, out);
            }
        }
        ExprKind::Method { obj, args, .. } => {
            collect_records(program, obj, k, out);
            for a in args {
                collect_records(program, a, k, out);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_records(program, lhs, k, out);
            collect_records(program, rhs, k, out);
        }
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) => collect_records(program, expr, k, out),
        ExprKind::Table(fields) => {
            for f in fields {
                match f {
                    TableField::Positional(v) | TableField::Named(_, v) => collect_records(program, v, k, out),
                    TableField::Keyed(a, b) => {
                        collect_records(program, a, k, out);
                        collect_records(program, b, k, out);
                    }
                }
            }
        }
        _ => {}
    }
    let _ = free_locals;
}

fn chunk_name(f: &SourceFile) -> String {
    match &f.path {
        Some(p) => format!("@{}", p.display()),
        None => format!("={}", f.chunk),
    }
}
