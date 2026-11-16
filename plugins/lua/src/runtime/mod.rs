//! LuaJIT ランタイム本体。
//!
//! 変数・関数・プロシージャをドット区切りの名前で Lua のグローバルに登録し、スクリプトを実行する。
//!
//! | モジュール | 内容 |
//! |---|---|
//! | `value` | [`Value`] / [`ValueType`] と Lua の値との変換 |
//! | `error` | [`Error`] / [`Result`] |
//! | `callback` | [`Callback`] と、そのエラーを Lua エラー越しに受け渡す仕組み |
//! | `path` | ドット区切りの名前での Lua グローバルの読み書き |
//! | `deferred` | 遅延プロシージャ(キュー本体は `queue.lua`) |

mod callback;
mod deferred;
mod error;
mod path;
mod value;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use mlua::{Lua, LuaOptions, MultiValue, StdLib, Value as LuaValue};

pub use callback::{Callback, CallbackError};
pub use error::{Error, Result};
pub use value::{Value, ValueType};

use callback::FailureStash;
use deferred::Deferred;
use value::{lua_to_value, value_to_lua};

/// LuaJIT のランタイム。
pub struct LuaRuntime {
    lua: Lua,
    vars: HashMap<String, ValueType>,
    names: HashSet<String>,
    deferred: Deferred,
    failures: Arc<FailureStash>,
}

impl fmt::Debug for LuaRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LuaRuntime")
            .field("vars", &self.vars)
            .field("names", &self.names)
            .field("deferred", &self.deferred.len())
            .finish_non_exhaustive()
    }
}

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaRuntime {
    pub fn new() -> Self {
        // SAFETY: 遅延プロシージャのキューに LuaJIT FFI を使うため、mlua が unsafe 扱いにしている FFI ライブラリを読み込む。
        let lua =
            unsafe { Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::FFI, LuaOptions::new()) };
        Self {
            lua,
            vars: HashMap::new(),
            names: HashSet::new(),
            deferred: Deferred::default(),
            failures: Arc::new(FailureStash::default()),
        }
    }

    /// 変数を追加する。初期値は `ty` に変換して設定する。
    pub fn add_variable(&mut self, name: &str, ty: ValueType, value: Value) -> Result<&mut Self> {
        let value = value.coerce(ty)?;
        self.register_name(name)?;
        path::set(&self.lua, name, value_to_lua(&self.lua, value)?)?;
        self.vars.insert(name.to_string(), ty);
        Ok(self)
    }

    /// 登録済み変数の型。
    pub fn variable_type(&self, name: &str) -> Option<ValueType> {
        self.vars.get(name).copied()
    }

    /// 登録済みの変数を更新する。
    pub fn set_variable(&mut self, name: &str, value: Value) -> Result<&mut Self> {
        let ty = self
            .variable_type(name)
            .ok_or_else(|| Error::UnknownVariable(name.to_string()))?;
        let value = value
            .coerce(ty)
            .map_err(|e| Error::Type(format!("variable '{name}': {e}")))?;
        path::set(&self.lua, name, value_to_lua(&self.lua, value)?)?;
        Ok(self)
    }

    /// 登録済みの変数の現在値(スクリプトが書き換えた値を含む)を宣言型で返す。
    pub fn get_variable(&self, name: &str) -> Result<Value> {
        let ty = self
            .variable_type(name)
            .ok_or_else(|| Error::UnknownVariable(name.to_string()))?;
        let value = path::get(&self.lua, name)?;
        lua_to_value(&self.lua, ty, value)
            .map_err(|m| Error::Type(format!("variable '{name}': {m}")))
    }

    /// 関数を追加する。Lua から呼ばれるたびに `callback` を呼び、戻り値を `ret` に変換して返す(`None` なら `nil`)。
    pub fn add_function(
        &mut self,
        name: &str,
        callback: Callback,
        ret: Option<ValueType>,
        arg_types: Vec<ValueType>,
    ) -> Result<&mut Self> {
        self.register_name(name)?;
        let fname = name.to_string();
        let failures = Arc::clone(&self.failures);
        let func = self.lua.create_function(move |lua, args: MultiValue| {
            if args.len() != arg_types.len() {
                return Err(mlua::Error::runtime(format!(
                    "{fname}: expected {} arguments, got {}",
                    arg_types.len(),
                    args.len()
                )));
            }
            let mut values = Vec::with_capacity(args.len());
            for (i, (arg, ty)) in args.into_iter().zip(&arg_types).enumerate() {
                let v = lua_to_value(lua, *ty, arg).map_err(|m| {
                    mlua::Error::runtime(format!("{fname}: argument #{}: {m}", i + 1))
                })?;
                values.push(v);
            }
            let result = callback(&values).map_err(|e| failures.stash(e))?;
            match ret {
                None => Ok(LuaValue::Nil),
                Some(ty) => {
                    let v = result
                        .coerce(ty)
                        .map_err(|e| mlua::Error::runtime(format!("{fname}: return value: {e}")))?;
                    value_to_lua(lua, v)
                }
            }
        })?;
        path::set(&self.lua, name, LuaValue::Function(func))?;
        Ok(self)
    }

    /// プロシージャを追加する。
    ///
    /// `realtime` が `true` なら戻り値の無い関数として即時に `callback` を呼ぶ。
    /// `false` なら呼び出しを FFI のキューに積み、[`run`](Self::run) でスクリプトが正常終了した後にまとめて実行する。
    pub fn add_procedure(
        &mut self,
        name: &str,
        callback: Callback,
        realtime: bool,
        arg_types: Vec<ValueType>,
    ) -> Result<&mut Self> {
        if realtime {
            return self.add_function(name, callback, None, arg_types);
        }
        self.register_name(name)?;
        let wrapper = self.deferred.add(&self.lua, name, callback, arg_types)?;
        path::set(&self.lua, name, LuaValue::Function(wrapper))?;
        Ok(self)
    }

    /// スクリプトを実行し、正常終了したら遅延プロシージャを呼び出し順に実行する。
    /// スクリプトがエラーで終わった場合、積まれた遅延プロシージャは実行せずに破棄する。
    pub fn run(&mut self, script: &str) -> Result<&mut Self> {
        self.failures.clear();
        self.deferred.reset()?;
        let result = self.lua.load(script).set_name("=script").exec();
        let callback_error = result.as_ref().err().and_then(|e| self.failures.take(e));
        self.failures.clear();
        if let Err(e) = result {
            self.deferred.reset()?;
            return Err(match callback_error {
                Some(original) => Error::Callback(original),
                None => Error::Lua(e),
            });
        }
        self.deferred.flush()?;
        Ok(self)
    }

    fn register_name(&mut self, name: &str) -> Result<()> {
        path::split_name(name)?;
        if !self.names.insert(name.to_string()) {
            return Err(Error::InvalidName(format!(
                "'{name}' is already registered"
            )));
        }
        Ok(())
    }
}
