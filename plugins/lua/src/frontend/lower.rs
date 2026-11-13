//! full-moon の AST → HIR。スコープ解決と、独自関数(spec)呼び出しの静的解決を行う。

use full_moon::ast::{self, Call, Expression, FunctionArgs, Index, LastStmt, Prefix, Suffix, Var};
use full_moon::node::Node;
use full_moon::tokenizer::{StringLiteralQuoteType, Symbol, TokenReference, TokenType};

use crate::diag::Diagnostics;
use crate::frontend::literal::{self, NumberLit};
use crate::hir::*;
use crate::spec::Registry;

/// 1 ファイル分の lowering の状態。
pub(crate) struct Lowerer<'a> {
    pub program: &'a mut Program,
    registry: &'a Registry,
    diags: &'a mut Diagnostics,
    file: FileId,
    scopes: Vec<Vec<(String, VarId)>>,
    func_stack: Vec<FuncId>,
    /// 定数文字列で呼ばれた require / dofile(あとでファイルを解決する)。
    pub requires: Vec<(String, Span)>,
    pub dofiles: Vec<(String, Span)>,
}

fn ident(tok: &TokenReference) -> String {
    match tok.token_type() {
        TokenType::Identifier { identifier } => identifier.to_string(),
        _ => tok.token().to_string(),
    }
}

impl<'a> Lowerer<'a> {
    pub fn new(
        program: &'a mut Program,
        registry: &'a Registry,
        diags: &'a mut Diagnostics,
        file: FileId,
    ) -> Self {
        Self {
            program,
            registry,
            diags,
            file,
            scopes: Vec::new(),
            func_stack: Vec::new(),
            requires: Vec::new(),
            dofiles: Vec::new(),
        }
    }

    // ---- 位置・診断 -------------------------------------------------------------

    fn span(&self, node: &impl Node) -> Span {
        let start = node.start_position();
        let end = node.end_position();
        match start {
            Some(s) => Span {
                file: self.file,
                start: s.bytes() as u32,
                end: end.map(|e| e.bytes() as u32).unwrap_or(s.bytes() as u32),
                line: s.line() as u32,
                col: s.character() as u32,
            },
            None => Span {
                file: self.file,
                ..Span::default()
            },
        }
    }

    fn error(&mut self, code: &'static str, span: Span, msg: impl Into<String>) {
        let loc = self.program.location(span);
        self.diags.error(code, loc, msg);
    }

    // ---- スコープ ---------------------------------------------------------------

    fn cur_func(&self) -> FuncId {
        *self.func_stack.last().expect("no function")
    }

    fn declare(&mut self, name: String, kind: VarKind, span: Span) -> VarId {
        let id = self.program.vars.len() as VarId;
        self.program.vars.push(VarInfo {
            name: name.clone(),
            kind,
            func: self.cur_func(),
            span,
            captured: false,
            reassigned: false,
        });
        self.scopes
            .last_mut()
            .expect("no scope")
            .push((name, id));
        id
    }

    fn lookup(&mut self, name: &str) -> Option<VarId> {
        let found = self
            .scopes
            .iter()
            .rev()
            .find_map(|s| s.iter().rev().find(|(n, _)| n == name).map(|(_, v)| *v))?;
        if self.program.vars[found as usize].func != self.cur_func() {
            self.program.vars[found as usize].captured = true;
        }
        Some(found)
    }

    fn next_stmt_id(&mut self) -> StmtId {
        let id = self.program.stmt_count;
        self.program.stmt_count += 1;
        id
    }

    fn stmt(&mut self, kind: StmtKind, span: Span) -> Stmt {
        let id = self.next_stmt_id();
        Stmt { id, kind, span }
    }

    // ---- 関数・ブロック ---------------------------------------------------------

    /// ファイルのトップレベルチャンクを lowering する。
    pub fn lower_chunk(&mut self, block: &ast::Block, span: Span) -> FuncId {
        let id = self.new_func(span, Some("main chunk".into()), true);
        self.func_stack.push(id);
        self.scopes.push(Vec::new());
        let body = self.block_stmts(block);
        self.scopes.pop();
        self.func_stack.pop();
        let f = &mut self.program.funcs[id as usize];
        f.is_vararg = true;
        f.body = body;
        id
    }

    fn new_func(&mut self, span: Span, name: Option<String>, is_chunk: bool) -> FuncId {
        let id = self.program.funcs.len() as FuncId;
        self.program.funcs.push(Function {
            id,
            params: Vec::new(),
            is_vararg: false,
            body: Block::default(),
            span,
            parent: self.func_stack.last().copied(),
            name,
            is_chunk,
        });
        id
    }

    fn function(
        &mut self,
        body: &ast::FunctionBody,
        name: Option<String>,
        method: bool,
        span: Span,
    ) -> FuncId {
        let id = self.new_func(span, name, false);
        self.func_stack.push(id);
        self.scopes.push(Vec::new());
        let mut params = Vec::new();
        if method {
            params.push(self.declare("self".into(), VarKind::Param, span));
        }
        let mut is_vararg = false;
        for p in body.parameters().iter() {
            match p {
                ast::Parameter::Name(tok) => {
                    let s = self.span(tok);
                    params.push(self.declare(ident(tok), VarKind::Param, s));
                }
                ast::Parameter::Ellipsis(_) => is_vararg = true,
                #[allow(unreachable_patterns)]
                _ => {
                    let s = self.span(p);
                    self.error("unsupported-syntax", s, "unsupported parameter syntax");
                }
            }
        }
        let block = self.block(body.block());
        self.scopes.pop();
        self.func_stack.pop();
        let f = &mut self.program.funcs[id as usize];
        f.params = params;
        f.is_vararg = is_vararg;
        f.body = block;
        id
    }

    /// 新しいスコープを作ってブロックを lowering する。
    fn block(&mut self, block: &ast::Block) -> Block {
        self.scopes.push(Vec::new());
        let b = self.block_stmts(block);
        self.scopes.pop();
        b
    }

    /// 現在のスコープのままブロックの文を lowering する。
    fn block_stmts(&mut self, block: &ast::Block) -> Block {
        let mut stmts = Vec::new();
        for s in block.stmts() {
            if let Some(st) = self.statement(s) {
                stmts.push(st);
            }
        }
        if let Some(last) = block.last_stmt() {
            let span = self.span(last);
            match last {
                LastStmt::Break(_) => {
                    let st = self.stmt(StmtKind::Break, span);
                    stmts.push(st);
                }
                LastStmt::Return(ret) => {
                    let exprs = ret.returns().iter().map(|e| self.expr(e)).collect();
                    let st = self.stmt(StmtKind::Return(exprs), span);
                    stmts.push(st);
                }
                #[allow(unreachable_patterns)]
                _ => self.error("unsupported-syntax", span, "unsupported last statement"),
            }
        }
        Block { stmts }
    }

    // ---- 文 ---------------------------------------------------------------------

    fn statement(&mut self, s: &ast::Stmt) -> Option<Stmt> {
        let span = self.span(s);
        let kind = match s {
            ast::Stmt::LocalAssignment(la) => {
                let exprs: Vec<Expr> = la.expressions().iter().map(|e| self.expr(e)).collect();
                let vars = la
                    .names()
                    .iter()
                    .map(|n| {
                        let sp = self.span(n);
                        self.declare(ident(n), VarKind::Local, sp)
                    })
                    .collect();
                StmtKind::Local { vars, exprs }
            }
            ast::Stmt::Assignment(a) => {
                let exprs: Vec<Expr> = a.expressions().iter().map(|e| self.expr(e)).collect();
                let targets = a.variables().iter().map(|v| self.lvalue(v)).collect();
                StmtKind::Assign { targets, exprs }
            }
            ast::Stmt::FunctionCall(fc) => {
                let e = self.chain(fc.prefix(), &fc.suffixes().collect::<Vec<_>>(), span);
                StmtKind::Call(e)
            }
            ast::Stmt::Do(d) => StmtKind::Do(self.block(d.block())),
            ast::Stmt::While(w) => {
                let cond = self.expr(w.condition());
                let body = self.block(w.block());
                StmtKind::While { cond, body }
            }
            ast::Stmt::Repeat(r) => {
                // until の条件は本体のスコープで評価される。
                self.scopes.push(Vec::new());
                let body = self.block_stmts(r.block());
                let cond = self.expr(r.until());
                self.scopes.pop();
                StmtKind::Repeat { body, cond }
            }
            ast::Stmt::If(i) => {
                let mut arms = vec![(self.expr(i.condition()), self.block(i.block()))];
                if let Some(elifs) = i.else_if() {
                    for e in elifs {
                        arms.push((self.expr(e.condition()), self.block(e.block())));
                    }
                }
                let else_block = i.else_block().map(|b| self.block(b));
                StmtKind::If { arms, else_block }
            }
            ast::Stmt::NumericFor(nf) => {
                let start = self.expr(nf.start());
                let limit = self.expr(nf.end());
                let step = nf.step().map(|e| self.expr(e));
                self.scopes.push(Vec::new());
                let vs = self.span(nf.index_variable());
                let var = self.declare(ident(nf.index_variable()), VarKind::ForIndex, vs);
                let body = self.block(nf.block());
                self.scopes.pop();
                StmtKind::NumFor {
                    var,
                    start,
                    limit,
                    step,
                    body,
                }
            }
            ast::Stmt::GenericFor(gf) => {
                let exprs = gf.expressions().iter().map(|e| self.expr(e)).collect();
                self.scopes.push(Vec::new());
                let vars = gf
                    .names()
                    .iter()
                    .map(|n| {
                        let sp = self.span(n);
                        self.declare(ident(n), VarKind::ForVar, sp)
                    })
                    .collect();
                let body = self.block(gf.block());
                self.scopes.pop();
                StmtKind::GenFor { vars, exprs, body }
            }
            ast::Stmt::LocalFunction(lf) => {
                let ns = self.span(lf.name());
                let name = ident(lf.name());
                let var = self.declare(name.clone(), VarKind::LocalFunction, ns);
                let func = self.function(lf.body(), Some(name), false, span);
                StmtKind::LocalFunction { var, func }
            }
            ast::Stmt::FunctionDeclaration(fd) => return self.function_decl(fd, span),
            ast::Stmt::Goto(g) => StmtKind::Goto(ident(g.label_name())),
            ast::Stmt::Label(l) => StmtKind::Label(ident(l.name())),
            #[allow(unreachable_patterns)]
            _ => {
                self.error("unsupported-syntax", span, "unsupported statement");
                return None;
            }
        };
        Some(self.stmt(kind, span))
    }

    /// `function a.b:c(...) end` → `a.b.c = function(self, ...) end`
    fn function_decl(&mut self, fd: &ast::FunctionDeclaration, span: Span) -> Option<Stmt> {
        let fname = fd.name();
        let names: Vec<&TokenReference> = fname.names().iter().collect();
        let method = fname.method_name();
        let mut display = names.iter().map(|t| ident(t)).collect::<Vec<_>>().join(".");
        if let Some(m) = method {
            display.push(':');
            display.push_str(&ident(m));
        }
        let first = names[0];
        let first_span = self.span(first);
        let first_name = ident(first);

        let target = if names.len() == 1 && method.is_none() {
            match self.lookup(&first_name) {
                Some(v) => {
                    self.program.vars[v as usize].reassigned = true;
                    LValue::Local(v, first_span)
                }
                None => {
                    self.check_reserved_global(&first_name, first_span);
                    LValue::Global(first_name, first_span)
                }
            }
        } else {
            let mut obj = self.name_expr(&first_name, first_span);
            let mut keys: Vec<&TokenReference> = names[1..].to_vec();
            if let Some(m) = method {
                keys.push(m);
            }
            let last = keys.pop().expect("at least one key");
            for k in keys {
                let ks = self.span(k);
                obj = Expr {
                    kind: ExprKind::Index {
                        obj: Box::new(obj),
                        key: Box::new(Expr {
                            kind: ExprKind::Str(ident(k).into_bytes()),
                            span: ks,
                        }),
                    },
                    span: ks,
                };
            }
            let ks = self.span(last);
            if self.registry.get(&display.replace(':', ".")).is_some()
                && matches!(obj.kind, ExprKind::Global(_) | ExprKind::Index { .. })
                && self.path_of(&obj).is_some()
            {
                self.error(
                    "spec-reassign",
                    ks,
                    format!("`{}` is a registered function and cannot be redefined", display),
                );
            }
            LValue::Index {
                obj,
                key: Expr {
                    kind: ExprKind::Str(ident(last).into_bytes()),
                    span: ks,
                },
                span: ks,
            }
        };
        let func = self.function(fd.body(), Some(display), method.is_some(), span);
        let fexpr = Expr {
            kind: ExprKind::Function(func),
            span,
        };
        Some(self.stmt(
            StmtKind::Assign {
                targets: vec![target],
                exprs: vec![fexpr],
            },
            span,
        ))
    }

    /// グローバル変数の参照式からドット区切りのパスを得る(`obj.a.b`)。
    fn path_of(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Global(n) => Some(n.clone()),
            ExprKind::Index { obj, key } => match &key.kind {
                ExprKind::Str(s) => Some(format!(
                    "{}.{}",
                    self.path_of(obj)?,
                    String::from_utf8_lossy(s)
                )),
                _ => None,
            },
            _ => None,
        }
    }

    fn check_reserved_global(&mut self, name: &str, span: Span) {
        if self.registry.root_names().any(|r| r == name) {
            self.error(
                "spec-reserved",
                span,
                format!("global `{name}` is reserved for registered functions and cannot be assigned"),
            );
        }
    }

    fn lvalue(&mut self, v: &Var) -> LValue {
        let span = self.span(v);
        match v {
            Var::Name(tok) => {
                let name = ident(tok);
                match self.lookup(&name) {
                    Some(id) => {
                        self.program.vars[id as usize].reassigned = true;
                        LValue::Local(id, span)
                    }
                    None => {
                        self.check_reserved_global(&name, span);
                        LValue::Global(name, span)
                    }
                }
            }
            Var::Expression(ve) => {
                let suffixes: Vec<&Suffix> = ve.suffixes().collect();
                let (last, rest) = suffixes.split_last().expect("var expression without suffix");
                let obj = self.chain(ve.prefix(), rest, span);
                let key = match last {
                    Suffix::Index(Index::Dot { name, .. }) => {
                        let ks = self.span(name);
                        Expr {
                            kind: ExprKind::Str(ident(name).into_bytes()),
                            span: ks,
                        }
                    }
                    Suffix::Index(Index::Brackets { expression, .. }) => self.expr(expression),
                    _ => {
                        self.error("unsupported-syntax", span, "cannot assign to a call");
                        Expr {
                            kind: ExprKind::Nil,
                            span,
                        }
                    }
                };
                // 登録済みの spec のパスへの代入は禁止(property は読み取り専用)。
                if let (Some(p), ExprKind::Str(k)) = (self.path_of(&obj), &key.kind) {
                    let full = format!("{p}.{}", String::from_utf8_lossy(k));
                    if let Some(entry) = self.registry.get(&full) {
                        let msg = if self.registry.is_property(entry) {
                            format!("property `{full}` is read-only")
                        } else {
                            format!("`{full}` is a registered function and cannot be reassigned")
                        };
                        self.error("spec-reassign", span, msg);
                    }
                }
                LValue::Index { obj, key, span }
            }
            #[allow(unreachable_patterns)]
            _ => {
                self.error("unsupported-syntax", span, "unsupported assignment target");
                LValue::Global("_".into(), span)
            }
        }
    }

    // ---- 式 ---------------------------------------------------------------------

    fn name_expr(&mut self, name: &str, span: Span) -> Expr {
        let kind = match self.lookup(name) {
            Some(v) => ExprKind::Local(v),
            None => ExprKind::Global(name.to_string()),
        };
        Expr { kind, span }
    }

    pub fn expr(&mut self, e: &Expression) -> Expr {
        let span = self.span(e);
        let kind = match e {
            Expression::BinaryOperator { lhs, binop, rhs } => {
                let op = match binop {
                    ast::BinOp::Plus(_) => BinOp::Add,
                    ast::BinOp::Minus(_) => BinOp::Sub,
                    ast::BinOp::Star(_) => BinOp::Mul,
                    ast::BinOp::Slash(_) => BinOp::Div,
                    ast::BinOp::Percent(_) => BinOp::Mod,
                    ast::BinOp::Caret(_) => BinOp::Pow,
                    ast::BinOp::TwoDots(_) => BinOp::Concat,
                    ast::BinOp::TwoEqual(_) => BinOp::Eq,
                    ast::BinOp::TildeEqual(_) => BinOp::Ne,
                    ast::BinOp::LessThan(_) => BinOp::Lt,
                    ast::BinOp::LessThanEqual(_) => BinOp::Le,
                    ast::BinOp::GreaterThan(_) => BinOp::Gt,
                    ast::BinOp::GreaterThanEqual(_) => BinOp::Ge,
                    ast::BinOp::And(_) => BinOp::And,
                    ast::BinOp::Or(_) => BinOp::Or,
                    #[allow(unreachable_patterns)]
                    _ => {
                        self.error("unsupported-syntax", span, "unsupported binary operator");
                        BinOp::Add
                    }
                };
                ExprKind::Binary {
                    op,
                    lhs: Box::new(self.expr(lhs)),
                    rhs: Box::new(self.expr(rhs)),
                }
            }
            Expression::Parentheses { expression, .. } => {
                ExprKind::Paren(Box::new(self.expr(expression)))
            }
            Expression::UnaryOperator { unop, expression } => {
                let op = match unop {
                    ast::UnOp::Minus(_) => UnOp::Neg,
                    ast::UnOp::Not(_) => UnOp::Not,
                    ast::UnOp::Hash(_) => UnOp::Len,
                    #[allow(unreachable_patterns)]
                    _ => {
                        self.error("unsupported-syntax", span, "unsupported unary operator");
                        UnOp::Neg
                    }
                };
                ExprKind::Unary {
                    op,
                    expr: Box::new(self.expr(expression)),
                }
            }
            Expression::Function(af) => {
                ExprKind::Function(self.function(af.body(), None, false, span))
            }
            Expression::FunctionCall(fc) => {
                return self.chain(fc.prefix(), &fc.suffixes().collect::<Vec<_>>(), span)
            }
            Expression::TableConstructor(tc) => ExprKind::Table(self.table(tc)),
            Expression::Number(tok) => self.number(tok, span),
            Expression::String(tok) => ExprKind::Str(self.string(tok, span)),
            Expression::Symbol(tok) => match tok.token_type() {
                TokenType::Symbol { symbol } => match symbol {
                    Symbol::True => ExprKind::Bool(true),
                    Symbol::False => ExprKind::Bool(false),
                    Symbol::Nil => ExprKind::Nil,
                    Symbol::Ellipsis => ExprKind::Vararg,
                    _ => {
                        self.error("unsupported-syntax", span, "unexpected symbol");
                        ExprKind::Nil
                    }
                },
                _ => ExprKind::Nil,
            },
            Expression::Var(v) => match v {
                Var::Name(tok) => return self.name_or_spec(tok, span),
                Var::Expression(ve) => {
                    return self.chain(ve.prefix(), &ve.suffixes().collect::<Vec<_>>(), span)
                }
                #[allow(unreachable_patterns)]
                _ => {
                    self.error("unsupported-syntax", span, "unsupported variable syntax");
                    ExprKind::Nil
                }
            },
            #[allow(unreachable_patterns)]
            _ => {
                self.error("unsupported-syntax", span, "unsupported expression");
                ExprKind::Nil
            }
        };
        Expr { kind, span }
    }

    /// 名前単体の参照。登録済みの名前(ドットなし)の property ならその読み取りにする。
    fn name_or_spec(&mut self, tok: &TokenReference, span: Span) -> Expr {
        let name = ident(tok);
        if self.lookup(&name).is_none() {
            if let Some(entry) = self.registry.get(&name) {
                if self.registry.is_property(entry) {
                    let start = self.span(tok).start;
                    return self.new_site(name, vec![], false, true, span, start, 0, span.end);
                }
                if self.registry.py(entry).is_none() {
                    self.error(
                        "spec-not-called",
                        span,
                        format!("`{name}` is a registered function and must be called directly"),
                    );
                }
            }
        }
        self.name_expr(&name, span)
    }

    fn number(&mut self, tok: &TokenReference, span: Span) -> ExprKind {
        let text = match tok.token_type() {
            TokenType::Number { text } => text.to_string(),
            _ => tok.token().to_string(),
        };
        match literal::parse_number(&text) {
            Some(NumberLit::Number(v)) => ExprKind::Number(v),
            Some(NumberLit::CData) => ExprKind::CData(text),
            None => {
                self.error("invalid-number", span, format!("malformed number `{text}`"));
                ExprKind::Number(0.0)
            }
        }
    }

    fn string(&mut self, tok: &TokenReference, span: Span) -> Vec<u8> {
        match tok.token_type() {
            TokenType::StringLiteral {
                literal, quote_type, ..
            } => match quote_type {
                StringLiteralQuoteType::Brackets => literal::long_string(literal),
                _ => match literal::unescape(literal) {
                    Ok(b) => b,
                    Err(msg) => {
                        self.error("invalid-string", span, msg);
                        Vec::new()
                    }
                },
            },
            _ => Vec::new(),
        }
    }

    fn table(&mut self, tc: &ast::TableConstructor) -> Vec<TableField> {
        let mut fields = Vec::new();
        for f in tc.fields().iter() {
            match f {
                ast::Field::NoKey(e) => fields.push(TableField::Positional(self.expr(e))),
                ast::Field::NameKey { key, value, .. } => {
                    fields.push(TableField::Named(ident(key), self.expr(value)))
                }
                ast::Field::ExpressionKey { key, value, .. } => {
                    fields.push(TableField::Keyed(self.expr(key), self.expr(value)))
                }
                #[allow(unreachable_patterns)]
                _ => {
                    let s = self.span(f);
                    self.error("unsupported-syntax", s, "unsupported table field");
                }
            }
        }
        fields
    }

    fn args(&mut self, args: &FunctionArgs) -> Vec<Expr> {
        match args {
            FunctionArgs::Parentheses { arguments, .. } => {
                arguments.iter().map(|e| self.expr(e)).collect()
            }
            FunctionArgs::String(tok) => {
                let span = self.span(tok);
                let s = self.string(tok, span);
                vec![Expr {
                    kind: ExprKind::Str(s),
                    span,
                }]
            }
            FunctionArgs::TableConstructor(tc) => {
                let span = self.span(tc.as_ref());
                vec![Expr {
                    kind: ExprKind::Table(self.table(tc)),
                    span,
                }]
            }
            #[allow(unreachable_patterns)]
            _ => {
                let span = self.span(args);
                self.error("unsupported-syntax", span, "unsupported call arguments");
                Vec::new()
            }
        }
    }

    /// `prefix suffix*` のチェーン(呼び出し・添字)を lowering する。
    fn chain(&mut self, prefix: &Prefix, suffixes: &[&Suffix], span: Span) -> Expr {
        let (mut base, start) = match self.spec_chain(prefix, suffixes, span) {
            Some(r) => r,
            None => {
                let base = match prefix {
                    // spec としての解釈は spec_chain で済んでいる(エラーもそちらで報告済み)。
                    Prefix::Name(tok) => {
                        let s = self.span(tok);
                        self.name_expr(&ident(tok), s)
                    }
                    Prefix::Expression(e) => self.expr(e),
                    #[allow(unreachable_patterns)]
                    _ => {
                        self.error("unsupported-syntax", span, "unsupported prefix");
                        Expr {
                            kind: ExprKind::Nil,
                            span,
                        }
                    }
                };
                (base, 0)
            }
        };
        for s in &suffixes[start..] {
            base = self.apply_suffix(base, s);
        }
        base
    }

    fn apply_suffix(&mut self, base: Expr, s: &Suffix) -> Expr {
        let span = Span {
            end: self.span(s).end,
            ..base.span
        };
        let kind = match s {
            Suffix::Index(Index::Dot { name, .. }) => {
                let ks = self.span(name);
                ExprKind::Index {
                    obj: Box::new(base),
                    key: Box::new(Expr {
                        kind: ExprKind::Str(ident(name).into_bytes()),
                        span: ks,
                    }),
                }
            }
            Suffix::Index(Index::Brackets { expression, .. }) => ExprKind::Index {
                obj: Box::new(base),
                key: Box::new(self.expr(expression)),
            },
            Suffix::Call(Call::AnonymousCall(args)) => {
                let args = self.args(args);
                if let ExprKind::Global(g) = &base.kind {
                    if let [Expr {
                        kind: ExprKind::Str(s),
                        span: arg_span,
                    }] = args.as_slice()
                    {
                        let name = String::from_utf8_lossy(s).into_owned();
                        match g.as_str() {
                            "require" => self.requires.push((name, *arg_span)),
                            "dofile" => {
                                self.program.dofile_calls.push(DofileCall {
                                    path: name.clone(),
                                    span,
                                });
                                self.dofiles.push((name, *arg_span))
                            }
                            _ => {}
                        }
                    }
                }
                ExprKind::Call {
                    func: Box::new(base),
                    args,
                }
            }
            Suffix::Call(Call::MethodCall(mc)) => ExprKind::Method {
                obj: Box::new(base),
                name: ident(mc.name()),
                args: self.args(mc.args()),
            },
            #[allow(unreachable_patterns)]
            _ => {
                self.error("unsupported-syntax", span, "unsupported suffix");
                return base;
            }
        };
        Expr { kind, span }
    }

    /// チェーンの先頭が登録済みの spec のパスなら、その呼び出し(property の読み取り)を作る。
    /// 戻り値は (spec の式, 消費した suffix の数)。
    fn spec_chain(
        &mut self,
        prefix: &Prefix,
        suffixes: &[&Suffix],
        span: Span,
    ) -> Option<(Expr, usize)> {
        let Prefix::Name(tok) = prefix else {
            return None;
        };
        let root = ident(tok);
        if self.lookup(&root).is_some() {
            return None;
        }
        let chain_start = self.span(tok).start;
        let mut path = root;
        let mut i = 0usize;
        loop {
            if let Some(entry) = self.registry.get(&path).cloned() {
                // path そのものが spec。
                if self.registry.is_property(&entry) {
                    let sp = self.chain_span(span, suffixes, i);
                    let callee_end = if i == 0 {
                        self.span(tok).end
                    } else {
                        self.span(suffixes[i - 1]).end
                    };
                    let e = self.new_site(path, vec![], false, true, sp, chain_start, i as u32, callee_end);
                    return Some((e, i));
                }
                match suffixes.get(i) {
                    Some(Suffix::Call(Call::AnonymousCall(fargs))) => {
                        let callee_end = self.span(fargs.as_ref()).start;
                        let args = self.args(fargs);
                        let sp = self.chain_span(span, suffixes, i + 1);
                        let e = self.new_site(
                            path,
                            args,
                            false,
                            false,
                            sp,
                            chain_start,
                            (i + 1) as u32,
                            callee_end,
                        );
                        return Some((e, i + 1));
                    }
                    _ => {
                        if self.registry.py(&entry).is_some() {
                            // PyFunction は実体のある関数なので、値として使ってもよい。
                            return None;
                        }
                        let sp = self.chain_span(span, suffixes, i);
                        self.error(
                            "spec-not-called",
                            sp,
                            format!("`{path}` is a registered function and must be called directly"),
                        );
                        return None;
                    }
                }
            }
            match suffixes.get(i) {
                Some(Suffix::Index(Index::Dot { name, .. })) => {
                    path = format!("{path}.{}", ident(name));
                    i += 1;
                }
                Some(Suffix::Call(Call::MethodCall(mc))) => {
                    let cand = format!("{path}.{}", ident(mc.name()));
                    let entry = self.registry.get(&cand).cloned()?;
                    if self.registry.is_property(&entry) {
                        let sp = self.chain_span(span, suffixes, i + 1);
                        self.error(
                            "spec-not-callable",
                            sp,
                            format!("property `{cand}` cannot be called"),
                        );
                        return None;
                    }
                    let args = self.args(mc.args());
                    let sp = self.chain_span(span, suffixes, i + 1);
                    let callee_end = self.span(mc.args()).start;
                    let e = self.new_site(cand, args, true, false, sp, chain_start, (i + 1) as u32, callee_end);
                    return Some((e, i + 1));
                }
                _ => return None,
            }
        }
    }

    /// チェーンの先頭から `count` 個の suffix までの範囲。
    fn chain_span(&self, span: Span, suffixes: &[&Suffix], count: usize) -> Span {
        if count == 0 {
            return span;
        }
        match suffixes.get(count - 1) {
            Some(s) => Span {
                end: self.span(*s).end,
                ..span
            },
            None => span,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new_site(
        &mut self,
        name: String,
        args: Vec<Expr>,
        is_method: bool,
        property: bool,
        span: Span,
        chain_start: u32,
        suffixes_consumed: u32,
        callee_end: u32,
    ) -> Expr {
        let id = self.program.sites.len() as SiteId;
        self.program.sites.push(Site {
            id,
            name,
            args,
            is_method,
            property,
            span,
            chain_start,
            suffixes_consumed,
            callee_end,
        });
        Expr {
            kind: ExprKind::Spec(id),
            span,
        }
    }
}
