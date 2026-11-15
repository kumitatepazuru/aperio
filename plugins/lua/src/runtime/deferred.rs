//! 遅延プロシージャ。
//!
//! Lua 側では LuaJIT FFI の配列(キュー、`queue.lua`)に `[proc_id, 引数...]` を積むだけにし、
//! スクリプトが正常終了した後に Rust がその配列をアドレス経由で読み、呼び出し順通りに [`Callback`] を実行する。
//! キューは全遅延プロシージャで共有するので、プロシージャをまたいでも呼び出し順が保たれる。

use std::fmt::Write;

use mlua::{Function, Lua, Table, Value as LuaValue};

use super::callback::Callback;
use super::error::{Error, Result};
use super::value::{Value, ValueType, I64_LOWER, I64_UPPER};

/// キューの Lua 実装。
const QUEUE_LUA: &str = include_str!("queue.lua");

/// キューの 1 要素
#[repr(C)]
#[derive(Clone, Copy)]
union Slot {
    f: f64,
    i: i64,
}

const _: () = assert!(std::mem::size_of::<Slot>() == 8);

/// 読み込み済みの `queue.lua` のモジュールテーブルと、よく使う関数。
struct Queue {
    table: Table,
    state: Function,
    reset: Function,
}

struct DeferredProc {
    arg_types: Vec<ValueType>,
    callback: Callback,
}

/// 登録済みの遅延プロシージャとキュー。
#[derive(Default)]
pub(super) struct Deferred {
    /// インデックスがキュー上の `proc_id`。
    procs: Vec<DeferredProc>,
    /// 最初の遅延プロシージャ登録時に読み込む。
    queue: Option<Queue>,
}

impl Deferred {
    pub(super) fn len(&self) -> usize {
        self.procs.len()
    }

    /// 遅延プロシージャを登録し、Lua から呼ぶラッパー関数を返す。
    pub(super) fn add(
        &mut self,
        lua: &Lua,
        name: &str,
        callback: Callback,
        arg_types: Vec<ValueType>,
    ) -> Result<Function> {
        let queue = self.queue(lua)?.table.clone();
        let id = self.procs.len();
        let wrapper: Function = lua
            .load(wrapper_source(&arg_types))
            .set_name(format!("={name}"))
            .call((queue, id, name))?;
        self.procs.push(DeferredProc {
            arg_types,
            callback,
        });
        Ok(wrapper)
    }

    fn queue(&mut self, lua: &Lua) -> Result<&Queue> {
        if self.queue.is_none() {
            let table: Table = lua.load(QUEUE_LUA).set_name("=lua_runtime.queue").eval()?;
            let state = table.raw_get("state")?;
            let reset = table.raw_get("reset")?;
            self.queue = Some(Queue {
                table,
                state,
                reset,
            });
        }
        Ok(self.queue.as_ref().expect("queue is initialized above"))
    }

    /// キューを空にする。
    pub(super) fn reset(&self) -> Result<()> {
        if let Some(q) = &self.queue {
            q.reset.call::<()>(())?;
        }
        Ok(())
    }

    /// キューに積まれた呼び出しを順に実行し、キューを空にする。
    /// コールバックがエラーを返したらそこで中断する。
    pub(super) fn flush(&self) -> Result<()> {
        let Some(queue) = &self.queue else {
            return Ok(());
        };
        let (addr, len, strs, nstr): (f64, usize, Table, usize) = queue.state.call(())?;
        let decoded = self.read_queue(addr as usize, len, &strs, nstr);
        queue.reset.call::<()>(())?;
        let (slots, strings) = decoded?;

        let mut args = Vec::new();
        let mut pos = 0;
        while pos < slots.len() {
            // SAFETY: Slot は f64 / i64 の union で、どのビット列も両方の型として有効。
            // proc_id・引数の数・文字列インデックスは read_queue で検証済み。
            let proc = &self.procs[unsafe { slots[pos].i } as usize];
            let end = pos + 1 + proc.arg_types.len();
            args.clear();
            for (slot, ty) in slots[pos + 1..end].iter().zip(&proc.arg_types) {
                // SAFETY: 同上。
                args.push(unsafe {
                    match ty {
                        ValueType::Float => Value::Float(slot.f),
                        ValueType::Int => Value::Int(slot.i),
                        ValueType::Bool => Value::Bool(slot.i != 0),
                        ValueType::Str => Value::Str(strings[slot.i as usize - 1].clone()),
                    }
                });
            }
            (proc.callback)(&args).map_err(Error::Callback)?;
            pos = end;
        }
        Ok(())
    }

    /// キューの FFI 配列と文字列表を Rust 側にコピーし、内容を検証する。
    ///
    /// コールバックの実行中に Lua 側の配列を参照し続けないよう、配列はここでコピーする。
    fn read_queue(
        &self,
        addr: usize,
        len: usize,
        strs: &Table,
        nstr: usize,
    ) -> Result<(Vec<Slot>, Vec<String>)> {
        let corrupted = || {
            Error::Lua(mlua::Error::runtime(
                "deferred procedure queue is corrupted",
            ))
        };
        if len == 0 {
            return Ok((Vec::new(), Vec::new()));
        }
        if addr == 0 || !addr.is_multiple_of(std::mem::align_of::<Slot>()) {
            return Err(corrupted());
        }
        // SAFETY: addr / len は queue.lua の state() が返した、Q のクロージャが参照し続けている
        // `aperio_lua_slot[cap]` (cap >= len) の先頭アドレスと使用数。LuaJIT の cdata は GC で移動しないし、
        // コピーが終わるまで Lua コードは実行しないので、配列は解放・再確保されない。
        let slots = unsafe { std::slice::from_raw_parts(addr as *const Slot, len) }.to_vec();

        let strings = (1..=nstr)
            .map(|i| match strs.raw_get::<LuaValue>(i)? {
                LuaValue::String(s) => Ok(s.to_string_lossy()),
                _ => Err(corrupted()),
            })
            .collect::<Result<Vec<_>>>()?;

        let mut pos = 0;
        while pos < len {
            // SAFETY: Slot は f64 / i64 の union で、どのビット列も両方の型として有効。
            let id = usize::try_from(unsafe { slots[pos].i }).map_err(|_| corrupted())?;
            let proc = self.procs.get(id).ok_or_else(corrupted)?;
            let end = pos + 1 + proc.arg_types.len();
            if end > len {
                return Err(corrupted());
            }
            for (slot, ty) in slots[pos + 1..end].iter().zip(&proc.arg_types) {
                // SAFETY: 同上。
                let index = unsafe { slot.i };
                if *ty == ValueType::Str && !(1..=nstr as i64).contains(&index) {
                    return Err(corrupted());
                }
            }
            pos = end;
        }
        Ok((slots, strings))
    }
}

/// 遅延プロシージャの Lua 側ラッパーを生成する。
/// チャンクには `(Q, proc_id, name)` を渡し、引数の型チェックとキューへの書き込みを行う関数を返す。
/// 型チェックの規則とエラーメッセージは [`lua_to_value`](super::value::lua_to_value) と揃えている。
fn wrapper_source(arg_types: &[ValueType]) -> String {
    let n = arg_types.len();
    let params: Vec<String> = (1..=n).map(|i| format!("a{i}")).collect();
    let mut src = String::new();
    src.push_str("local Q, id, name = ...\n");
    src.push_str("local reserve, str = Q.reserve, Q.str\n");
    src.push_str("local type, tostring, error, select = type, tostring, error, select\n");
    let mut sig = params.join(", ");
    if !sig.is_empty() {
        sig.push_str(", ");
    }
    let _ = writeln!(src, "return function({sig}...)");
    let _ = writeln!(
        src,
        "  local extra = select('#', ...)\n  if extra ~= 0 then error(name .. ': expected {n} arguments, got ' .. ({n} + extra), 2) end"
    );
    for (i, ty) in arg_types.iter().enumerate() {
        let a = &params[i];
        let idx = i + 1;
        let mismatch = format!(
            "error(name .. ': argument #{idx}: expected {}, got ' .. (type({a}) == 'number' and tostring({a}) or type({a})), 2)",
            ty.name()
        );
        let check = match ty {
            ValueType::Float => format!("  if type({a}) ~= 'number' then {mismatch} end"),
            ValueType::Int => format!(
                "  if type({a}) ~= 'number' or {a} % 1 ~= 0 or {a} < {I64_LOWER:?} or {a} >= {I64_UPPER:?} then {mismatch} end"
            ),
            ValueType::Str => format!(
                "  if type({a}) == 'number' then {a} = tostring({a}) elseif type({a}) ~= 'string' then {mismatch} end"
            ),
            ValueType::Bool => format!("  if type({a}) ~= 'boolean' then {mismatch} end"),
        };
        src.push_str(&check);
        src.push('\n');
    }
    let _ = writeln!(src, "  local b, s = reserve({})", n + 1);
    src.push_str("  b[s].i = id\n");
    for (i, ty) in arg_types.iter().enumerate() {
        let a = &params[i];
        let k = i + 1;
        let store = match ty {
            ValueType::Float => format!("  b[s + {k}].f = {a}"),
            ValueType::Int => format!("  b[s + {k}].i = {a}"),
            ValueType::Str => format!("  b[s + {k}].i = str({a})"),
            ValueType::Bool => format!("  b[s + {k}].i = {a} and 1 or 0"),
        };
        src.push_str(&store);
        src.push('\n');
    }
    src.push_str("end\n");
    src
}
