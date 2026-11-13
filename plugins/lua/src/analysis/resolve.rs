//! 呼び出し先・値の静的解決。
//!
//! 次のものを、どの関数(または組み込み)を指すかに解決する:
//! - 標準ライブラリ(`math.sin`、`print`)。そのグローバル / フィールドへの代入がプログラム中に無い場合のみ。
//! - local 関数 / 再代入されない local に入った関数・組み込み(`local sin = math.sin`)。
//! - プログラム全体で 1 回だけ関数を代入されるグローバル(`function foo() end`)。
//! - 再代入されない local テーブルのフィールドで、1 回だけ関数を代入されるもの(`function M.f() end`)、
//!   およびそれを `return M` するモジュールを require した結果のフィールド。
//!
//! setfenv / getfenv / module の使用や `_G` への書き込みがあると、グローバルの解決は行わない。

use std::collections::{HashMap, HashSet};

use crate::hir::*;
use crate::stdlib::{self, Builtin, Kind};

/// 静的に分かる値。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StaticValue {
    Func(FuncId),
    Builtin(&'static Builtin),
    /// 再代入されない local に入ったテーブルコンストラクタ(フィールド解決に使う)。
    LocalTable(VarId),
    /// require で読み込まれる(同梱済みの)モジュール。
    Module(FileId),
}

pub struct Resolver<'a> {
    pub program: &'a Program,
    /// 代入のあるグローバル名。
    assigned_globals: HashSet<String>,
    /// 1 回だけ関数を代入されたグローバル。
    global_funcs: HashMap<String, FuncId>,
    /// `lib.name = ...` のように書き換えられた組み込みのフィールド。
    assigned_lib_fields: HashSet<String>,
    /// local テーブルのフィールドへの代入: (テーブル変数, フィールド名) → 代入された式(複数なら None)。
    table_fields: HashMap<(VarId, String), Option<Expr>>,
    /// 宣言時の値: local 変数 → 初期化式。
    local_inits: HashMap<VarId, LocalInit>,
    /// グローバルが動的に扱われる(setfenv / getfenv / module / _G への書き込み)。
    dynamic_globals: bool,
}

#[derive(Clone, Debug)]
enum LocalInit {
    Expr(Expr),
    Func(FuncId),
}

impl<'a> Resolver<'a> {
    pub fn new(program: &'a Program) -> Self {
        let mut r = Resolver {
            program,
            assigned_globals: HashSet::new(),
            global_funcs: HashMap::new(),
            assigned_lib_fields: HashSet::new(),
            table_fields: HashMap::new(),
            local_inits: HashMap::new(),
            dynamic_globals: false,
        };
        let mut global_assign_count: HashMap<String, (u32, Option<FuncId>)> = HashMap::new();
        for f in &program.funcs {
            r.scan_block(&f.body, &mut global_assign_count);
        }
        for (name, (count, func)) in global_assign_count {
            r.assigned_globals.insert(name.clone());
            if let (1, Some(f)) = (count, func) {
                r.global_funcs.insert(name, f);
            }
        }
        r
    }

    fn scan_block(&mut self, b: &Block, globals: &mut HashMap<String, (u32, Option<FuncId>)>) {
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Local { vars, exprs } => {
                    for (i, v) in vars.iter().enumerate() {
                        if let Some(e) = exprs.get(i) {
                            let multi_tail = i + 1 == exprs.len() && e.is_multi(self.program);
                            if !(multi_tail && i + 1 < vars.len()) {
                                self.local_inits.insert(*v, LocalInit::Expr(e.clone()));
                            }
                        }
                    }
                    for e in exprs {
                        self.scan_expr(e);
                    }
                }
                StmtKind::LocalFunction { var, func } => {
                    self.local_inits.insert(*var, LocalInit::Func(*func));
                }
                StmtKind::Assign { targets, exprs } => {
                    for (i, t) in targets.iter().enumerate() {
                        let value = exprs.get(i).filter(|_| i < exprs.len());
                        match t {
                            LValue::Global(name, _) => {
                                let entry = globals.entry(name.clone()).or_insert((0, None));
                                entry.0 += 1;
                                entry.1 = match value.map(|e| &e.kind) {
                                    Some(ExprKind::Function(f)) => Some(*f),
                                    _ => None,
                                };
                                if name == "_G" {
                                    self.dynamic_globals = true;
                                }
                            }
                            LValue::Index { obj, key, .. } => {
                                match (&obj.kind, &key.kind) {
                                    (ExprKind::Global(g), ExprKind::Str(k)) => {
                                        let field = format!("{g}.{}", String::from_utf8_lossy(k));
                                        if g == "_G" {
                                            self.dynamic_globals = true;
                                        }
                                        self.assigned_lib_fields.insert(field);
                                    }
                                    (ExprKind::Global(g), _) if g == "_G" => {
                                        self.dynamic_globals = true;
                                    }
                                    (ExprKind::Local(v), ExprKind::Str(k)) => {
                                        let k = String::from_utf8_lossy(k).into_owned();
                                        let slot = self.table_fields.entry((*v, k)).or_insert(None);
                                        // 2 回目以降の代入があれば解決しない(None のまま印を付ける)。
                                        *slot = match slot {
                                            None => value.cloned().or(Some(Expr {
                                                kind: ExprKind::Nil,
                                                span: t.span(),
                                            })),
                                            Some(_) => Some(Expr {
                                                kind: ExprKind::Vararg,
                                                span: t.span(),
                                            }),
                                        };
                                    }
                                    _ => {}
                                }
                                self.scan_expr(obj);
                                self.scan_expr(key);
                            }
                            LValue::Local(..) => {}
                        }
                    }
                    for e in exprs {
                        self.scan_expr(e);
                    }
                }
                StmtKind::Call(e) => self.scan_expr(e),
                StmtKind::Do(b) => self.scan_block(b, globals),
                StmtKind::While { cond, body } | StmtKind::Repeat { body, cond } => {
                    self.scan_expr(cond);
                    self.scan_block(body, globals);
                }
                StmtKind::If { arms, else_block } => {
                    for (c, b) in arms {
                        self.scan_expr(c);
                        self.scan_block(b, globals);
                    }
                    if let Some(b) = else_block {
                        self.scan_block(b, globals);
                    }
                }
                StmtKind::NumFor {
                    start,
                    limit,
                    step,
                    body,
                    ..
                } => {
                    self.scan_expr(start);
                    self.scan_expr(limit);
                    if let Some(s) = step {
                        self.scan_expr(s);
                    }
                    self.scan_block(body, globals);
                }
                StmtKind::GenFor { exprs, body, .. } => {
                    for e in exprs {
                        self.scan_expr(e);
                    }
                    self.scan_block(body, globals);
                }
                StmtKind::Return(es) => {
                    for e in es {
                        self.scan_expr(e);
                    }
                }
                _ => {}
            }
        }
    }

    fn scan_expr(&mut self, e: &Expr) {
        // setfenv / getfenv / module はグローバル環境を動的に変える。
        if let ExprKind::Global(g) = &e.kind {
            if matches!(g.as_str(), "setfenv" | "getfenv" | "module") {
                self.dynamic_globals = true;
            }
        }
        if let ExprKind::Index { obj, key } = &e.kind {
            if let (ExprKind::Global(g), ExprKind::Str(k)) = (&obj.kind, &key.kind) {
                if g == "debug" && (k == b"setfenv" || k == b"getfenv") {
                    self.dynamic_globals = true;
                }
            }
        }
        let program = self.program;
        walk_expr_children(program, e, &mut |c| self.scan_expr(c));
    }

    /// グローバルが動的に扱われているか(このとき組み込み・グローバル関数の解決をしない)。
    pub fn has_dynamic_globals(&self) -> bool {
        self.dynamic_globals
    }

    /// 式が静的に指す値。
    pub fn value_of(&self, e: &Expr) -> Option<StaticValue> {
        self.value_of_depth(e, 0)
    }

    fn value_of_depth(&self, e: &Expr, depth: u32) -> Option<StaticValue> {
        if depth > 32 {
            return None;
        }
        match &e.kind {
            ExprKind::Function(f) => Some(StaticValue::Func(*f)),
            ExprKind::Paren(inner) => self.value_of_depth(inner, depth + 1),
            ExprKind::Global(g) => {
                if self.dynamic_globals {
                    return None;
                }
                if !self.assigned_globals.contains(g) {
                    return stdlib::lookup(g).map(StaticValue::Builtin);
                }
                self.global_funcs.get(g).map(|f| StaticValue::Func(*f))
            }
            ExprKind::Local(v) => {
                let info = self.program.var(*v);
                if info.reassigned {
                    return None;
                }
                match self.local_inits.get(v)? {
                    LocalInit::Func(f) => Some(StaticValue::Func(*f)),
                    LocalInit::Expr(init) => match &init.kind {
                        ExprKind::Table(_) => Some(StaticValue::LocalTable(*v)),
                        _ => self.value_of_depth(init, depth + 1),
                    },
                }
            }
            ExprKind::Index { obj, key } => {
                let ExprKind::Str(k) = &key.kind else {
                    return None;
                };
                let k = String::from_utf8_lossy(k);
                match self.value_of_depth(obj, depth + 1)? {
                    StaticValue::Builtin(lib) if lib.kind == Kind::Library => {
                        let name = format!("{}.{k}", lib.name);
                        if self.assigned_lib_fields.contains(&name) {
                            return None;
                        }
                        stdlib::lookup(&name).map(StaticValue::Builtin)
                    }
                    StaticValue::LocalTable(t) => self.table_field(t, &k, depth),
                    StaticValue::Module(file) => {
                        let t = self.module_table(file)?;
                        self.table_field(t, &k, depth)
                    }
                    _ => None,
                }
            }
            ExprKind::Call { func, args } => {
                // require("name") で同梱済みのモジュール。
                if let (ExprKind::Global(g), [arg]) = (&func.kind, args.as_slice()) {
                    if g == "require" && !self.assigned_globals.contains("require") {
                        if let ExprKind::Str(name) = &arg.kind {
                            let name = String::from_utf8_lossy(name);
                            return self.program.modules.get(name.as_ref()).map(|f| StaticValue::Module(*f));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn table_field(&self, table: VarId, key: &str, depth: u32) -> Option<StaticValue> {
        // コンストラクタ内の `name = value` と、後からの 1 回だけの代入のどちらか一方のみを許す。
        let ctor = match self.local_inits.get(&table)? {
            LocalInit::Expr(Expr {
                kind: ExprKind::Table(fields),
                ..
            }) => fields.iter().find_map(|f| match f {
                TableField::Named(n, v) if n == key => Some(v.clone()),
                _ => None,
            }),
            _ => None,
        };
        let assigned = self.table_fields.get(&(table, key.to_string())).cloned();
        let value = match (ctor, assigned) {
            (Some(v), None) => v,
            (None, Some(Some(v))) if !matches!(v.kind, ExprKind::Vararg) => v,
            _ => return None,
        };
        self.value_of_depth(&value, depth + 1)
    }

    /// モジュールのチャンクが `return M`(M は local テーブル)なら M。
    fn module_table(&self, file: FileId) -> Option<VarId> {
        let main = self.program.files.get(file as usize)?.main;
        let body = &self.program.func(main).body;
        match body.stmts.last().map(|s| &s.kind) {
            Some(StmtKind::Return(r)) if r.len() == 1 => match self.value_of(&r[0])? {
                StaticValue::LocalTable(v) => Some(v),
                _ => None,
            },
            _ => None,
        }
    }

    /// 呼び出し式の呼び出し先。
    pub fn callee(&self, call: &Expr) -> Option<StaticValue> {
        match &call.kind {
            ExprKind::Call { func, .. } => self.value_of(func),
            ExprKind::Method { obj, name, .. } => {
                let key = Expr {
                    kind: ExprKind::Str(name.clone().into_bytes()),
                    span: call.span,
                };
                self.value_of(&Expr {
                    kind: ExprKind::Index {
                        obj: obj.clone(),
                        key: Box::new(key),
                    },
                    span: call.span,
                })
            }
            _ => None,
        }
    }
}

/// 式の直接の子に `f` を適用する(spec 呼び出しの引数も含む)。
pub fn walk_expr_children(program: &Program, e: &Expr, f: &mut impl FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Spec(site) => program.site(*site).args.iter().for_each(&mut *f),
        ExprKind::Index { obj, key } => {
            f(obj);
            f(key);
        }
        ExprKind::Call { func, args } => {
            f(func);
            args.iter().for_each(&mut *f);
        }
        ExprKind::Method { obj, args, .. } => {
            f(obj);
            args.iter().for_each(&mut *f);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) => f(expr),
        ExprKind::Table(fields) => {
            for fl in fields {
                match fl {
                    TableField::Positional(v) | TableField::Named(_, v) => f(v),
                    TableField::Keyed(k, v) => {
                        f(k);
                        f(v);
                    }
                }
            }
        }
        _ => {}
    }
}
