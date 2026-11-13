//! 実行時: CPU 用 Lua を LuaJIT で実行し、カーネルの params をバイト列に詰める。
//!
//! - 実行ごとに新しい環境テーブル(`__index = _G`)で実行する。スクリプトが代入したグローバル変数は
//!   次の実行に持ち越されない(標準ライブラリのテーブル自体を書き換えた場合は持ち越される)。
//! - 外部値は `__lg_ext`、PyFunction は spec の名前のパスに置く。
//! - 同梱モジュールは実行ごとに読み込み直す(`require` を環境ごとの実装に差し替える)。
//! - `__lg_dispatch(k, values)` でレイアウトに従ってバイト列に詰め、コールバックに渡す。

pub mod pack;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use indexmap::IndexMap;
use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::codegen::layout::{decode_header, KernelLayout};
use crate::spec::{FunctionSpec, Registry};
use crate::types::TypeSpec;

/// 変換・実行で共通に使う LuaJIT の状態を作る。
///
/// サンドボックスはしない(計画の決定事項)ので、debug / ffi / jit を含むすべての標準ライブラリを読み込む。
pub fn new_lua_state() -> mlua::Lua {
    // SAFETY: debug / ffi を含む全ライブラリを開く。スクリプトは信頼されたもの(ユーザー自身のもの)として扱う。
    unsafe { mlua::Lua::unsafe_new_with(mlua::StdLib::ALL, mlua::LuaOptions::default()) }
}

/// 実行時のヘルパー(グローバルに 1 回だけ定義する)。
const RUNTIME_PRELUDE: &str = r#"
function __lg_count(s, e, st)
  local s0, e0, t0 = tonumber(s), tonumber(e), tonumber(st)
  if s0 == nil then error("'for' initial value must be a number", 2) end
  if e0 == nil then error("'for' limit must be a number", 2) end
  if t0 == nil then error("'for' step must be a number", 2) end
  if t0 == 0 then error("lua_gpu: a parallelized 'for' loop must not have a step of 0", 2) end
  local n
  if t0 > 0 then n = math.floor((e0 - s0) / t0) + 1 else n = math.floor((s0 - e0) / -t0) + 1 end
  if n ~= n or n < 0 then n = 0 end
  if n > 4294967295 then error("lua_gpu: too many iterations in a parallelized loop", 2) end
  return n
end
function __lg_r(t, i, v) t[i] = v return v end
function __lg_gpu_only(name)
  error("lua_gpu: GPU function '" .. name .. "' cannot be called on the CPU", 2)
end
function __lg_overflow(max)
  error("lua_gpu: an inner loop recorded more than " .. max .. " iterations (raise max_values_per_iteration)", 2)
end
"#;

/// 実行ごとに環境の中で実行する初期化(同梱モジュール用の require)。
const ENV_PRELUDE: &str = r#"
__lg_pre = {}
__lg_loaded = {}
local real_require = require
require = function(name)
  local v = __lg_loaded[name]
  if v ~= nil then return v end
  local f = __lg_pre[name]
  if f ~= nil then
    local r = f(name)
    if r == nil then r = true end
    __lg_loaded[name] = r
    return r
  end
  return real_require(name)
end
"#;

/// 外部値。
#[derive(Clone, Debug, PartialEq)]
pub enum ExtValue {
    Number(f64),
    Bool(bool),
    String(String),
    Array(Vec<ExtValue>),
    Record(IndexMap<String, ExtValue>),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RunError {
    /// 外部値の不足・型違い。
    #[error("{0}")]
    Arg(String),
    /// レイアウトのヘッダーが読めない、または params の値がレイアウトに合わない。
    #[error("{0}")]
    Layout(String),
    /// Lua の実行時エラー。
    #[error("{0}")]
    Lua(String),
}

/// ディスパッチを受け取るコールバック(カーネル番号, params のバイト列)。
pub type DispatchFn<'a> = dyn FnMut(u32, Vec<u8>) -> Result<(), String> + 'a;

struct Cached {
    layouts: Vec<KernelLayout>,
    main: Function,
}

pub struct Runtime {
    lua: Mutex<Inner>,
    externals: IndexMap<String, TypeSpec>,
}

struct Inner {
    lua: Lua,
    /// 環境に置くホスト関数(パス → 関数)。
    host: Vec<(Vec<String>, Function)>,
    /// spec の名前の先頭(グローバルのテーブルとして用意する)。
    roots: Vec<String>,
    chunks: HashMap<u64, Cached>,
    loads: HashMap<u64, Function>,
}

fn hash_of(parts: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}

impl Runtime {
    pub fn new(registry: &Registry) -> Result<Self, RunError> {
        let lua = new_lua_state();
        lua.load(RUNTIME_PRELUDE)
            .set_name("=lua_gpu_runtime")
            .exec()
            .map_err(|e| RunError::Lua(e.to_string()))?;
        let mut host = Vec::new();
        for spec in &registry.functions {
            if let FunctionSpec::Py(p) = spec {
                let f = p.func.clone();
                let func = lua
                    .create_function(move |lua, args: MultiValue| f(lua, args))
                    .map_err(|e| RunError::Lua(e.to_string()))?;
                host.push((p.name.split('.').map(str::to_string).collect(), func));
            }
        }
        let roots = registry.root_names().map(str::to_string).collect();
        Ok(Self {
            lua: Mutex::new(Inner {
                lua,
                host,
                roots,
                chunks: HashMap::new(),
                loads: HashMap::new(),
            }),
            externals: registry.externals.clone(),
        })
    }

    /// 外部値の名前と型。
    pub fn externals(&self) -> &IndexMap<String, TypeSpec> {
        &self.externals
    }

    /// CPU 用 Lua を実行する。カーネルのディスパッチは実行順に `dispatch` に渡す。
    pub fn run(
        &self,
        code: &str,
        externals: &IndexMap<String, ExtValue>,
        dispatch: &mut DispatchFn,
    ) -> Result<(), RunError> {
        for (name, ty) in &self.externals {
            match externals.get(name) {
                Some(v) => check_ext(ty, v, name)?,
                None => return Err(RunError::Arg(format!("missing external value `{name}`"))),
            }
        }
        for name in externals.keys() {
            if !self.externals.contains_key(name) {
                return Err(RunError::Arg(format!("unknown external value `{name}`")));
            }
        }

        let mut guard = self.lua.lock().map_err(|_| RunError::Lua("runtime lock poisoned".into()))?;
        let inner = &mut *guard;
        let key = hash_of(&[code]);
        if !inner.chunks.contains_key(&key) {
            let header = decode_header(code).map_err(RunError::Layout)?;
            // 行番号は元のスクリプトと同じなので、元のチャンク名で報告する。
            let main = inner
                .lua
                .load(code)
                .set_name(format!("={}", header.chunk))
                .into_function()
                .map_err(|e| RunError::Lua(e.to_string()))?;
            inner.chunks.insert(
                key,
                Cached {
                    layouts: header.kernels,
                    main,
                },
            );
        }
        let Inner {
            lua,
            host,
            roots,
            chunks,
            loads,
        } = inner;
        let cached = &chunks[&key];
        let layouts = &cached.layouts;
        let lua: &Lua = lua;

        let result: mlua::Result<()> = lua.scope(|scope| {
            let env = lua.create_table()?;
            let meta = lua.create_table()?;
            meta.set("__index", lua.globals())?;
            env.set_metatable(Some(meta))?;

            env.set("__lg_ext", ext_table(lua, externals)?)?;
            for r in roots.iter() {
                env.set(r.as_str(), lua.create_table()?)?;
            }
            for (path, f) in host.iter() {
                set_path(lua, &env, path, f.clone())?;
            }

            let env_for_load = env.clone();
            let load = scope.create_function_mut(move |lua, (src, name): (String, String)| {
                let k = hash_of(&[&src, &name]);
                let f = match loads.get(&k) {
                    Some(f) => f.clone(),
                    None => {
                        let f = lua.load(src.as_str()).set_name(name).into_function()?;
                        loads.insert(k, f.clone());
                        f
                    }
                };
                f.set_environment(env_for_load.clone())?;
                Ok(f)
            })?;
            env.set("__lg_load", load)?;

            let dispatch_fn = scope.create_function_mut(|_, (k, values): (u32, Table)| {
                let layout = layouts.get(k as usize).ok_or_else(|| {
                    mlua::Error::RuntimeError(format!("lua_gpu: unknown kernel {k}"))
                })?;
                let bytes = pack::pack(layout, &values).map_err(|e| {
                    mlua::Error::RuntimeError(format!("lua_gpu: kernel {k}: {e}"))
                })?;
                dispatch(k, bytes).map_err(mlua::Error::RuntimeError)
            })?;
            env.set("__lg_dispatch", dispatch_fn)?;

            lua.load(ENV_PRELUDE)
                .set_name("=lua_gpu_env")
                .set_environment(env.clone())
                .exec()?;

            cached.main.set_environment(env)?;
            cached.main.call::<()>(())
        });
        result.map_err(|e| RunError::Lua(e.to_string()))
    }
}

fn set_path(lua: &Lua, root: &Table, path: &[String], f: Function) -> mlua::Result<()> {
    let mut t = root.clone();
    for (i, k) in path.iter().enumerate() {
        if i + 1 == path.len() {
            t.set(k.as_str(), f.clone())?;
        } else {
            let next: Value = t.raw_get(k.as_str())?;
            t = match next {
                Value::Table(nt) => nt,
                _ => {
                    let nt = lua.create_table()?;
                    t.set(k.as_str(), nt.clone())?;
                    nt
                }
            };
        }
    }
    Ok(())
}

fn ext_table(lua: &Lua, externals: &IndexMap<String, ExtValue>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (k, v) in externals {
        t.set(k.as_str(), ext_to_lua(lua, v)?)?;
    }
    Ok(t)
}

pub fn ext_to_lua(lua: &Lua, v: &ExtValue) -> mlua::Result<Value> {
    Ok(match v {
        ExtValue::Number(n) => Value::Number(*n),
        ExtValue::Bool(b) => Value::Boolean(*b),
        ExtValue::String(s) => Value::String(lua.create_string(s)?),
        ExtValue::Array(items) => {
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, it) in items.iter().enumerate() {
                t.raw_set(i + 1, ext_to_lua(lua, it)?)?;
            }
            Value::Table(t)
        }
        ExtValue::Record(fields) => {
            let t = lua.create_table()?;
            for (k, it) in fields {
                t.raw_set(k.as_str(), ext_to_lua(lua, it)?)?;
            }
            Value::Table(t)
        }
    })
}

/// 外部値が宣言した型に合うか。
pub fn check_ext(ty: &TypeSpec, v: &ExtValue, path: &str) -> Result<(), RunError> {
    let bad = |want: &str| {
        Err(RunError::Arg(format!(
            "external `{path}` must be {want} (declared as {ty})"
        )))
    };
    match (ty, v) {
        (TypeSpec::Float, ExtValue::Number(_)) => Ok(()),
        (TypeSpec::Int, ExtValue::Number(n)) => {
            if n.fract() == 0.0 {
                Ok(())
            } else {
                bad("an integer")
            }
        }
        (TypeSpec::Bool, ExtValue::Bool(_)) => Ok(()),
        (TypeSpec::String, ExtValue::String(_)) => Ok(()),
        (TypeSpec::Any, _) => Ok(()),
        (TypeSpec::Array(elem, len), ExtValue::Array(items)) => {
            if let Some(n) = len {
                if items.len() != *n {
                    return Err(RunError::Arg(format!(
                        "external `{path}` must have exactly {n} elements (got {})",
                        items.len()
                    )));
                }
            }
            for (i, it) in items.iter().enumerate() {
                check_ext(elem, it, &format!("{path}[{}]", i + 1))?;
            }
            Ok(())
        }
        (TypeSpec::Record(fields), ExtValue::Record(vals)) => {
            for (k, t) in fields {
                match vals.get(k) {
                    Some(x) => check_ext(t, x, &format!("{path}.{k}"))?,
                    None => {
                        return Err(RunError::Arg(format!("external `{path}` is missing field `{k}`")))
                    }
                }
            }
            Ok(())
        }
        (TypeSpec::Float | TypeSpec::Int, _) => bad("a number"),
        (TypeSpec::Bool, _) => bad("a boolean"),
        (TypeSpec::String, _) => bad("a string"),
        (TypeSpec::Array(..), _) => bad("an array (list)"),
        (TypeSpec::Record(_), _) => bad("a record (dict)"),
    }
}
