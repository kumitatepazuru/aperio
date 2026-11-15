//! 関数・プロシージャのコールバックと、そのエラーを Lua エラー越しに受け渡す仕組み。
// TODO: 豪華なエラー実装なので、使わなさそうな部分は削って簡潔にする

use std::error::Error as StdError;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use super::value::Value;

/// [`Callback`] が返すエラー。Python バインディングでは `PyErr` がそのまま入る。
pub type CallbackError = Box<dyn StdError + Send + Sync>;

/// 関数・プロシージャの実体。引数は宣言した型に変換済みで渡される。
/// プロシージャや戻り値型 `None` の関数では戻り値は無視される。
pub type Callback = Arc<dyn Fn(&[Value]) -> Result<Value, CallbackError> + Send + Sync>;

/// Lua エラーの中に埋め込む、退避した [`CallbackError`] への目印。
#[derive(Debug)]
pub(super) struct CallbackFailure {
    index: usize,
    message: String,
}

impl fmt::Display for CallbackFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for CallbackFailure {}

/// [`Callback`] のエラーを Lua エラーに変換する間、元のエラーを退避しておく場所。
/// `run` が失敗したとき、Lua エラーの中の [`CallbackFailure`] から元のエラーを取り出す。
/// (スクリプトが `pcall` で握りつぶしたエラーを取り違えないよう、目印にインデックスを持たせる。)
#[derive(Default)]
pub(super) struct FailureStash(Mutex<Vec<Option<CallbackError>>>);

impl FailureStash {
    fn lock(&self) -> MutexGuard<'_, Vec<Option<CallbackError>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// エラーを退避し、目印を埋め込んだ Lua エラーを返す。
    pub(super) fn stash(&self, err: CallbackError) -> mlua::Error {
        let message = err.to_string();
        let mut errors = self.lock();
        let index = errors.len();
        errors.push(Some(err));
        mlua::Error::external(CallbackFailure { index, message })
    }

    /// Lua エラーに目印があれば、退避した元のエラーを取り出す。
    pub(super) fn take(&self, err: &mlua::Error) -> Option<CallbackError> {
        let failure = err.downcast_ref::<CallbackFailure>()?;
        self.lock().get_mut(failure.index).and_then(Option::take)
    }

    pub(super) fn clear(&self) {
        self.lock().clear();
    }
}
