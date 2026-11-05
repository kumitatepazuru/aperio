//! 解析用の中間表現(HIR)。
//!
//! full-moon の AST から、スコープ解決(local / upvalue / global)と糖衣構文の展開を済ませた木。
//! CPU 側の出力は元の AST をそのまま書き換えるので、HIR はカーネル化の解析と Slang 生成にだけ使う。
//! AST との対応はバイト位置([`Span`])で取る。

use std::path::PathBuf;

use indexmap::IndexMap;

use crate::diag::Location;

pub type FileId = u32;
pub type VarId = u32;
pub type FuncId = u32;
pub type StmtId = u32;
/// 独自関数(Cpu / Gpu / Py)の呼び出し箇所の番号。プログラム全体で通し番号。
pub type SiteId = u32;

/// ソース上の範囲。`start` / `end` はファイル内のバイト位置、`line` / `col` は 1 始まり。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn lua_symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "^",
            BinOp::Concat => "..",
            BinOp::Eq => "==",
            BinOp::Ne => "~=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }

    pub fn is_arith(self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
        )
    }

    pub fn is_compare(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Len,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Nil,
    Bool(bool),
    Number(f64),
    /// LuaJIT の cdata 数値リテラル(`1LL` / `2ULL` / `3i`)。CPU 専用。
    CData(String),
    Str(Vec<u8>),
    Vararg,
    Local(VarId),
    Global(String),
    Index {
        obj: Box<Expr>,
        key: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `obj:name(args)`。obj は 1 回だけ評価される。
    Method {
        obj: Box<Expr>,
        name: String,
        args: Vec<Expr>,
    },
    /// 独自関数(spec)の呼び出し、または property の読み取り。
    Spec(SiteId),
    Function(FuncId),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    /// 括弧。多値を 1 値に切り詰める。
    Paren(Box<Expr>),
    Table(Vec<TableField>),
}

impl Expr {
    /// 多値を返し得る式(関数呼び出しと `...`)か。
    pub fn is_multi(&self, program: &Program) -> bool {
        match &self.kind {
            ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Vararg => true,
            ExprKind::Spec(site) => !program.sites[*site as usize].property,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TableField {
    /// `{ v }`
    Positional(Expr),
    /// `{ name = v }`
    Named(String, Expr),
    /// `{ [k] = v }`
    Keyed(Expr, Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LValue {
    Local(VarId, Span),
    Global(String, Span),
    Index { obj: Expr, key: Expr, span: Span },
}

impl LValue {
    pub fn span(&self) -> Span {
        match self {
            LValue::Local(_, s) | LValue::Global(_, s) => *s,
            LValue::Index { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub id: StmtId,
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Local {
        vars: Vec<VarId>,
        exprs: Vec<Expr>,
    },
    Assign {
        targets: Vec<LValue>,
        exprs: Vec<Expr>,
    },
    /// 式文(関数呼び出し / メソッド呼び出し / spec 呼び出し)。
    Call(Expr),
    Do(Block),
    While {
        cond: Expr,
        body: Block,
    },
    /// `repeat body until cond`。cond は body のスコープで評価される。
    Repeat {
        body: Block,
        cond: Expr,
    },
    If {
        arms: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
    NumFor {
        var: VarId,
        start: Expr,
        limit: Expr,
        step: Option<Expr>,
        body: Block,
    },
    GenFor {
        vars: Vec<VarId>,
        exprs: Vec<Expr>,
        body: Block,
    },
    LocalFunction {
        var: VarId,
        func: FuncId,
    },
    Return(Vec<Expr>),
    Break,
    Goto(String),
    Label(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Local,
    Param,
    /// 数値 for のループ変数。
    ForIndex,
    /// 汎用 for の変数。
    ForVar,
    LocalFunction,
}

#[derive(Clone, Debug)]
pub struct VarInfo {
    pub name: String,
    pub kind: VarKind,
    /// 宣言した関数。
    pub func: FuncId,
    pub span: Span,
    /// 内側の関数から参照されているか(upvalue)。
    pub captured: bool,
    /// 宣言後に代入されることがあるか。
    pub reassigned: bool,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub id: FuncId,
    pub params: Vec<VarId>,
    pub is_vararg: bool,
    pub body: Block,
    pub span: Span,
    pub parent: Option<FuncId>,
    /// 表示用の名前(`function foo()` なら `foo`)。
    pub name: Option<String>,
    /// このファイルのトップレベルチャンクか。
    pub is_chunk: bool,
}

/// 独自関数の呼び出し箇所。
#[derive(Clone, Debug)]
pub struct Site {
    pub id: SiteId,
    /// spec の名前(`obj.getpixel`)。
    pub name: String,
    /// self を除いた引数。property の読み取りなら空。
    pub args: Vec<Expr>,
    pub is_method: bool,
    pub property: bool,
    pub span: Span,
    /// CPU 書き換え用: チェーンの先頭(prefix の名前トークン)のバイト位置と、
    /// spec に対応する部分が消費した suffix の数。
    pub chain_start: u32,
    pub suffixes_consumed: u32,
    /// CPU 書き換え用: 呼び出される側(`obj.getpixel` / `obj:getpixel`)の終わり = 引数の始まりの
    /// バイト位置。property の読み取りならチェーンの終わり。
    pub callee_end: u32,
}

/// 定数パスの dofile 呼び出し(CPU 書き換えで同梱したファイルの呼び出しに置き換える)。
#[derive(Clone, Debug)]
pub struct DofileCall {
    pub path: String,
    /// `dofile("x")` 全体の範囲。
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Main,
    /// `require(name)` で読み込まれるモジュール。
    Module,
    /// `dofile(path)` で読み込まれるファイル。
    Dofile,
    /// CpuFunction の生成関数が返したコードなど、コンバーター内部で作ったソース。
    Generated,
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: FileId,
    /// チャンク名(診断に出す名前)。
    pub chunk: String,
    pub path: Option<PathBuf>,
    pub source: String,
    pub kind: SourceKind,
    /// このファイルのトップレベル関数。
    pub main: FuncId,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub files: Vec<SourceFile>,
    pub funcs: Vec<Function>,
    pub vars: Vec<VarInfo>,
    pub sites: Vec<Site>,
    /// require の名前 → ファイル。
    pub modules: IndexMap<String, FileId>,
    /// dofile のパス(ソースに書かれた文字列)→ ファイル。
    pub dofiles: IndexMap<String, FileId>,
    /// 定数パスの dofile 呼び出し箇所。
    pub dofile_calls: Vec<DofileCall>,
    /// エントリポイント(メインチャンク)のファイル。
    pub main_file: FileId,
    pub stmt_count: u32,
}

impl Program {
    pub fn location(&self, span: Span) -> Location {
        let chunk = self
            .files
            .get(span.file as usize)
            .map(|f| f.chunk.clone())
            .unwrap_or_default();
        Location::new(chunk, span.line, span.col)
    }

    pub fn func(&self, id: FuncId) -> &Function {
        &self.funcs[id as usize]
    }

    pub fn var(&self, id: VarId) -> &VarInfo {
        &self.vars[id as usize]
    }

    pub fn site(&self, id: SiteId) -> &Site {
        &self.sites[id as usize]
    }

    pub fn main_func(&self) -> FuncId {
        self.files[self.main_file as usize].main
    }
}
