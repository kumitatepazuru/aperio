//! 実行時: CPU 用 Lua を LuaJIT で実行し、カーネルの params をバイト列に詰める。

/// 変換・実行で共通に使う LuaJIT の状態を作る。
///
/// サンドボックスはしない(計画の決定事項)ので、debug / ffi / jit を含むすべての標準ライブラリを読み込む。
pub fn new_lua_state() -> mlua::Lua {
    // SAFETY: debug / ffi を含む全ライブラリを開く。スクリプトは信頼されたもの(ユーザー自身のもの)として扱う。
    unsafe { mlua::Lua::unsafe_new_with(mlua::StdLib::ALL, mlua::LuaOptions::default()) }
}
