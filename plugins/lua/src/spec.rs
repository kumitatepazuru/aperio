//! 独自関数(FunctionSpec)と外部値の定義、および変換オプション。
//!
//! - [`FunctionSpec::Cpu`]: 変換時に生成関数を呼び、**Lua** のコードを得る。
//! - [`FunctionSpec::Gpu`]: 変換時に生成関数を呼び、**Slang** のコードを得る。
//! - [`FunctionSpec::Py`]: 実行時に Lua から直接呼ばれるホスト関数(CPU 専用)。

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::types::TypeSpec;

/// 変換時に分かっている定数値。
#[derive(Clone, Debug, PartialEq)]
pub enum ConstValue {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

/// 生成関数がどの文脈の呼び出し箇所に対して呼ばれたか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteContext {
    /// CPU で実行されるコード(Lua を返す)。
    Cpu,
    /// GPU カーネル内(Slang を返す)。
    Kernel,
}

impl SiteContext {
    pub fn as_str(self) -> &'static str {
        match self {
            SiteContext::Cpu => "cpu",
            SiteContext::Kernel => "kernel",
        }
    }
}

/// 生成関数に渡す、呼び出し箇所の情報。
#[derive(Clone, Debug, PartialEq)]
pub struct CallSite {
    /// 関数名(spec の name)。
    pub name: String,
    /// 対象言語での引数の式。コンバーターが先に仮引数へ束縛するので、識別子になる。
    pub args: Vec<String>,
    /// 変換時に定数と分かった引数の値。
    pub consts: Vec<Option<ConstValue>>,
    /// 推論した引数の型(型記法の文字列。不明なら `any`)。
    pub types: Vec<String>,
    /// `obj:foo()` 形式の呼び出しか(このとき args[0] が self)。
    pub is_method: bool,
    pub chunk: String,
    pub line: u32,
    pub col: u32,
    pub context: SiteContext,
}

/// 生成関数の戻り値。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gen {
    /// 戻り値ごとの式。空なら戻り値なし。
    pub exprs: Vec<String>,
    /// 式の前に実行する文。
    pub stmts: String,
    /// トップレベルの定義(Slang の関数や構造体など)。内容が同じものは 1 回だけ出力する。
    pub defs: String,
}

impl Gen {
    pub fn expr(e: impl Into<String>) -> Self {
        Self {
            exprs: vec![e.into()],
            ..Self::default()
        }
    }
}

/// 生成関数。`Ok(None)` または `Err` を返すと、その呼び出し箇所が変換エラーになる。
pub trait CodeGen: Send + Sync {
    fn generate(&self, site: &CallSite) -> Result<Option<Gen>, String>;
}

impl<F> CodeGen for F
where
    F: Fn(&CallSite) -> Result<Option<Gen>, String> + Send + Sync,
{
    fn generate(&self, site: &CallSite) -> Result<Option<Gen>, String> {
        self(site)
    }
}

/// 実行時に Lua から呼ばれるホスト関数。
pub type HostFn =
    Arc<dyn Fn(&mlua::Lua, mlua::MultiValue) -> mlua::Result<mlua::MultiValue> + Send + Sync>;

/// 外部値(毎回の実行時に値を渡すもの)。生成コード中では `${name}` で参照する。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSpec {
    pub name: String,
    pub ty: TypeSpec,
}

impl ExternalSpec {
    pub fn new(name: impl Into<String>, ty: TypeSpec) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

#[derive(Clone)]
pub struct CpuFunction {
    pub name: String,
    pub generate: Arc<dyn CodeGen>,
    /// 戻り値の型(任意)。分かっていればカーネル境界の型推論に使う。
    pub returns: Option<Vec<TypeSpec>>,
    pub externals: Vec<ExternalSpec>,
    /// true なら `obj.w` のように呼び出しなしで値として読む(読み取り専用)。
    pub property: bool,
}

#[derive(Clone)]
pub struct GpuFunction {
    pub name: String,
    pub generate: Arc<dyn CodeGen>,
    pub params: Vec<TypeSpec>,
    pub returns: Vec<TypeSpec>,
    pub externals: Vec<ExternalSpec>,
    pub property: bool,
}

#[derive(Clone)]
pub struct PyFunction {
    pub name: String,
    pub func: HostFn,
    pub returns: Option<Vec<TypeSpec>>,
}

#[derive(Clone)]
pub enum FunctionSpec {
    Cpu(CpuFunction),
    Gpu(GpuFunction),
    Py(PyFunction),
}

impl FunctionSpec {
    pub fn name(&self) -> &str {
        match self {
            FunctionSpec::Cpu(f) => &f.name,
            FunctionSpec::Gpu(f) => &f.name,
            FunctionSpec::Py(f) => &f.name,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            FunctionSpec::Cpu(_) => "CpuFunction",
            FunctionSpec::Gpu(_) => "GpuFunction",
            FunctionSpec::Py(_) => "PyFunction",
        }
    }

    fn externals(&self) -> &[ExternalSpec] {
        match self {
            FunctionSpec::Cpu(f) => &f.externals,
            FunctionSpec::Gpu(f) => &f.externals,
            FunctionSpec::Py(_) => &[],
        }
    }
}

impl fmt::Debug for FunctionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({:?})", self.kind(), self.name())
    }
}

/// 変換オプション。
#[derive(Clone, Debug)]
pub struct Options {
    /// require / dofile の解決に使う検索パス(`package.path` と同じ書式)。
    pub package_path: String,
    /// varying なループ(while / repeat など)の反復上限。超えたらループを抜ける。
    pub max_loop_iterations: u32,
    /// CPU での再生で、反復ごとに記録する値の上限(回数が変わる場合)。
    pub max_values_per_iteration: u32,
    /// カーネル内で作る可変長テーブルの上限。
    pub max_local_table_len: u32,
    /// tex0 以外に宣言する入力テクスチャの名前。
    pub extra_inputs: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            package_path: "./?.lua;./?/init.lua".to_string(),
            max_loop_iterations: 1_000_000,
            max_values_per_iteration: 16,
            max_local_table_len: 64,
            extra_inputs: Vec::new(),
        }
    }
}

/// spec の検証エラー。
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    #[error("invalid function name `{0}` (expected dotted Lua identifiers such as `obj.getpixel`)")]
    InvalidName(String),
    #[error("{kind} `{name}` is registered more than once")]
    Duplicate { kind: &'static str, name: String },
    #[error("`{0}` is registered both as PyFunction and as Cpu/GpuFunction")]
    PyConflict(String),
    #[error("`{name}` is a property in one spec and a function in another")]
    PropertyConflict { name: String },
    #[error("external `{name}` is declared with conflicting types `{a}` and `{b}`")]
    ExternalConflict { name: String, a: TypeSpec, b: TypeSpec },
    #[error("invalid external name `{0}`")]
    InvalidExternalName(String),
    #[error("GpuFunction `{name}`: type `{ty}` cannot be used on the GPU")]
    NonGpuType { name: String, ty: TypeSpec },
    #[error("GpuFunction `{name}` is a property but declares parameters")]
    PropertyWithParams { name: String },
    #[error("invalid extra input name `{0}`")]
    InvalidInputName(String),
}

/// 1 つの名前に登録された spec の組。
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub cpu: Option<usize>,
    pub gpu: Option<usize>,
    pub py: Option<usize>,
}

/// 検証済みの spec 一覧。
#[derive(Clone, Debug)]
pub struct Registry {
    pub functions: Vec<FunctionSpec>,
    pub by_name: IndexMap<String, Entry>,
    /// すべての spec の外部値を名前でまとめたもの(宣言順)。
    pub externals: IndexMap<String, TypeSpec>,
    pub options: Options,
}

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

pub fn is_lua_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric()) && !LUA_KEYWORDS.contains(&s)
}

impl Registry {
    pub fn new(functions: Vec<FunctionSpec>, options: Options) -> Result<Self, SpecError> {
        let mut by_name: IndexMap<String, Entry> = IndexMap::new();
        let mut externals: IndexMap<String, TypeSpec> = IndexMap::new();

        for input in &options.extra_inputs {
            if !is_lua_identifier(input) || input == "tex0" {
                return Err(SpecError::InvalidInputName(input.clone()));
            }
        }

        for (idx, spec) in functions.iter().enumerate() {
            let name = spec.name();
            if name.is_empty() || !name.split('.').all(is_lua_identifier) {
                return Err(SpecError::InvalidName(name.to_string()));
            }
            let entry = by_name.entry(name.to_string()).or_default();
            let slot = match spec {
                FunctionSpec::Cpu(_) => &mut entry.cpu,
                FunctionSpec::Gpu(_) => &mut entry.gpu,
                FunctionSpec::Py(_) => &mut entry.py,
            };
            if slot.is_some() {
                return Err(SpecError::Duplicate {
                    kind: spec.kind(),
                    name: name.to_string(),
                });
            }
            *slot = Some(idx);
            if entry.py.is_some() && (entry.cpu.is_some() || entry.gpu.is_some()) {
                return Err(SpecError::PyConflict(name.to_string()));
            }

            if let FunctionSpec::Gpu(g) = spec {
                for ty in g.params.iter().chain(&g.returns) {
                    if !ty.is_gpu_representable() {
                        return Err(SpecError::NonGpuType {
                            name: name.to_string(),
                            ty: ty.clone(),
                        });
                    }
                }
                if g.property && !g.params.is_empty() {
                    return Err(SpecError::PropertyWithParams {
                        name: name.to_string(),
                    });
                }
            }

            for ext in spec.externals() {
                if !is_lua_identifier(&ext.name) {
                    return Err(SpecError::InvalidExternalName(ext.name.clone()));
                }
                match externals.get(&ext.name) {
                    Some(prev) if *prev != ext.ty => {
                        return Err(SpecError::ExternalConflict {
                            name: ext.name.clone(),
                            a: prev.clone(),
                            b: ext.ty.clone(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        externals.insert(ext.name.clone(), ext.ty.clone());
                    }
                }
            }
        }

        // Cpu と Gpu の両方がある名前は、property かどうかが一致していなければならない。
        for (name, entry) in &by_name {
            if let (Some(c), Some(g)) = (entry.cpu, entry.gpu) {
                let (FunctionSpec::Cpu(c), FunctionSpec::Gpu(g)) = (&functions[c], &functions[g])
                else {
                    unreachable!()
                };
                if c.property != g.property {
                    return Err(SpecError::PropertyConflict { name: name.clone() });
                }
            }
        }

        Ok(Self {
            functions,
            by_name,
            externals,
            options,
        })
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name)
    }

    pub fn cpu(&self, entry: &Entry) -> Option<&CpuFunction> {
        match &self.functions[entry.cpu?] {
            FunctionSpec::Cpu(f) => Some(f),
            _ => None,
        }
    }

    pub fn gpu(&self, entry: &Entry) -> Option<&GpuFunction> {
        match &self.functions[entry.gpu?] {
            FunctionSpec::Gpu(f) => Some(f),
            _ => None,
        }
    }

    pub fn py(&self, entry: &Entry) -> Option<&PyFunction> {
        match &self.functions[entry.py?] {
            FunctionSpec::Py(f) => Some(f),
            _ => None,
        }
    }

    /// 名前の先頭要素(`obj.getpixel` なら `obj`)。グローバル変数として予約される。
    pub fn root_names(&self) -> impl Iterator<Item = &str> {
        let mut seen = indexmap::IndexSet::new();
        for name in self.by_name.keys() {
            seen.insert(name.split('.').next().unwrap());
        }
        seen.into_iter()
    }

    pub fn is_property(&self, entry: &Entry) -> bool {
        self.cpu(entry).is_some_and(|f| f.property) || self.gpu(entry).is_some_and(|f| f.property)
    }
}
