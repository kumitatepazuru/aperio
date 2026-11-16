//! ドット区切りの名前で Lua のグローバルを読み書きする。

use mlua::{Lua, Table, Value as LuaValue};

use super::error::{Error, Result};

/// 名前を `.` で分割する。空の要素があればエラー。
pub(super) fn split_name(name: &str) -> Result<Vec<&str>> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidName(format!("invalid name: '{name}'")));
    }
    Ok(parts)
}

/// 名前の位置に値を設定する。途中のテーブルが無ければ作る。
pub(super) fn set(lua: &Lua, name: &str, value: LuaValue) -> Result<()> {
    let parts = split_name(name)?;
    let parent = resolve_parent(lua, &parts, true)?;
    parent.raw_set(parts[parts.len() - 1], value)?;
    Ok(())
}

/// 名前の位置の値を取得する。
pub(super) fn get(lua: &Lua, name: &str) -> Result<LuaValue> {
    let parts = split_name(name)?;
    let parent = resolve_parent(lua, &parts, false)?;
    Ok(parent.raw_get(parts[parts.len() - 1])?)
}

/// `parts` の最後の要素を除いたテーブルの連鎖をグローバルからたどる。
fn resolve_parent(lua: &Lua, parts: &[&str], create: bool) -> Result<Table> {
    let mut table = lua.globals();
    for (i, part) in parts[..parts.len() - 1].iter().enumerate() {
        let value: LuaValue = table.raw_get(*part)?;
        table = match value {
            LuaValue::Table(t) => t,
            LuaValue::Nil if create => {
                let t = lua.create_table()?;
                table.raw_set(*part, t.clone())?;
                t
            }
            _ => {
                return Err(Error::InvalidName(format!(
                    "'{}' is not a table",
                    parts[..=i].join(".")
                )));
            }
        };
    }
    Ok(table)
}
