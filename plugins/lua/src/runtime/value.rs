//! 値と型、および Lua の値との相互変換。

use mlua::{Lua, Value as LuaValue};

use super::error::{Error, Result};

/// 変数・引数・戻り値として扱える型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    Float,
    Int,
    Str,
    Bool,
}

impl ValueType {
    /// Python 側の型名。
    pub fn name(self) -> &'static str {
        match self {
            ValueType::Float => "float",
            ValueType::Int => "int",
            ValueType::Str => "str",
            ValueType::Bool => "bool",
        }
    }
}

/// Lua と呼び出し元の間でやり取りする値。
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Float(f64),
    Int(i64),
    Str(String),
    Bool(bool),
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "None",
            Value::Float(_) => "float",
            Value::Int(_) => "int",
            Value::Str(_) => "str",
            Value::Bool(_) => "bool",
        }
    }

    /// 宣言型に合わせて変換する。`float` は `int` も受け付け、それ以外は型が一致する必要がある。
    pub fn coerce(self, ty: ValueType) -> Result<Value> {
        match (ty, self) {
            (ValueType::Float, Value::Float(f)) => Ok(Value::Float(f)),
            (ValueType::Float, Value::Int(i)) => Ok(Value::Float(i as f64)),
            (ValueType::Int, Value::Int(i)) => Ok(Value::Int(i)),
            (ValueType::Str, Value::Str(s)) => Ok(Value::Str(s)),
            (ValueType::Bool, Value::Bool(b)) => Ok(Value::Bool(b)),
            (ty, v) => Err(Error::Type(format!(
                "expected {}, got {}",
                ty.name(),
                v.type_name()
            ))),
        }
    }
}

/// int として扱える範囲(i64 に収まる整数値の double)。
pub(super) const I64_LOWER: f64 = -9_223_372_036_854_775_808.0;
pub(super) const I64_UPPER: f64 = 9_223_372_036_854_775_808.0;

/// Lua の値を宣言型に変換する。失敗時は Lua エラーメッセージ用の説明を返す。
///
/// float = number、int = 整数値の number、str = string(number は Lua の tostring と同じ規則で文字列化)、bool = boolean。
pub(super) fn lua_to_value(lua: &Lua, ty: ValueType, value: LuaValue) -> Result<Value, String> {
    let mismatch = |v: &LuaValue| match v {
        LuaValue::Number(n) => format!("expected {}, got {n}", ty.name()),
        _ => format!("expected {}, got {}", ty.name(), v.type_name()),
    };
    match (ty, value) {
        (ValueType::Float, LuaValue::Number(n)) => Ok(Value::Float(n)),
        (ValueType::Float, LuaValue::Integer(i)) => Ok(Value::Float(i as f64)),
        (ValueType::Int, LuaValue::Integer(i)) => Ok(Value::Int(i)),
        (ValueType::Int, LuaValue::Number(n))
            if n.fract() == 0.0 && (I64_LOWER..I64_UPPER).contains(&n) =>
        {
            Ok(Value::Int(n as i64))
        }
        (ValueType::Str, LuaValue::String(s)) => Ok(Value::Str(s.to_string_lossy())),
        (ValueType::Str, v @ (LuaValue::Number(_) | LuaValue::Integer(_))) => {
            match lua.coerce_string(v.clone()) {
                Ok(Some(s)) => Ok(Value::Str(s.to_string_lossy())),
                _ => Err(mismatch(&v)),
            }
        }
        (ValueType::Bool, LuaValue::Boolean(b)) => Ok(Value::Bool(b)),
        (_, v) => Err(mismatch(&v)),
    }
}

pub(super) fn value_to_lua(lua: &Lua, value: Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        Value::Nil => LuaValue::Nil,
        Value::Float(f) => LuaValue::Number(f),
        Value::Int(i) => LuaValue::Integer(i),
        Value::Str(s) => LuaValue::String(lua.create_string(s)?),
        Value::Bool(b) => LuaValue::Boolean(b),
    })
}
