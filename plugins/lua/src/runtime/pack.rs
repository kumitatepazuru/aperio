//! `__lg_dispatch(k, values)` に渡された Lua の値を、カーネルのレイアウトに従って 32 bit ワード列
//! (little-endian のバイト列)に詰める。

use mlua::{Table, Value};

use crate::codegen::layout::{Field, KernelLayout};

/// 値の検査・変換のエラー(どのフィールドかを含むメッセージ)。
pub type PackError = String;

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Nil => "nil",
        Value::Boolean(_) => "boolean",
        Value::Integer(_) | Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Table(_) => "table",
        Value::Function(_) => "function",
        _ => "userdata",
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

/// レイアウトに従って詰める。`values` は 1 始まりの配列(途中に nil があってもよい)。
pub fn pack(layout: &KernelLayout, values: &Table) -> Result<Vec<u8>, PackError> {
    let offsets = layout.offsets();
    let mut buf: Vec<u32> = vec![0; layout.fixed_words() as usize];
    // 記録ブロックの大きさに使う、反復の総数(最初のフィールド)。
    let mut total: u64 = 0;
    for (i, f) in layout.fields.iter().enumerate() {
        let v: Value = values
            .raw_get(i + 1)
            .map_err(|e| format!("param #{}: {e}", i + 1))?;
        if i == 0 {
            total = number(&v).unwrap_or(0.0).max(0.0) as u64;
        }
        write(f, &v, &mut buf, offsets[i] as usize, total, &format!("param #{}", i + 1))?;
    }
    if buf.is_empty() {
        buf.push(0);
    }
    let mut out = Vec::with_capacity(buf.len() * 4);
    for w in buf {
        out.extend_from_slice(&w.to_le_bytes());
    }
    Ok(out)
}

fn write(
    f: &Field,
    v: &Value,
    buf: &mut Vec<u32>,
    at: usize,
    total: u64,
    path: &str,
) -> Result<(), PackError> {
    match f {
        Field::F32 => {
            let n = number(v).ok_or_else(|| {
                format!("{path}: expected a number on the GPU but got {}", type_name(v))
            })?;
            buf[at] = (n as f32).to_bits();
        }
        Field::Bool => {
            buf[at] = match v {
                Value::Boolean(b) => *b as u32,
                Value::Nil => 0,
                _ => {
                    return Err(format!(
                        "{path}: expected a boolean on the GPU but got {}",
                        type_name(v)
                    ))
                }
            };
        }
        Field::U32 => {
            let n = number(v).ok_or_else(|| format!("{path}: expected a count but got {}", type_name(v)))?;
            if n < 0.0 || n > u32::MAX as f64 || n.fract() != 0.0 {
                return Err(format!("{path}: invalid count {n}"));
            }
            buf[at] = n as u32;
        }
        Field::Array(elem) => {
            let Value::Table(t) = v else {
                return Err(format!("{path}: expected a table (array) but got {}", type_name(v)));
            };
            let len = t.raw_len();
            let stride = elem.words() as usize;
            let off = buf.len();
            buf.resize(off + len * stride, 0);
            for i in 0..len {
                let ev: Value = t
                    .raw_get(i + 1)
                    .map_err(|e| format!("{path}[{}]: {e}", i + 1))?;
                write(elem, &ev, buf, off + i * stride, total, &format!("{path}[{}]", i + 1))?;
            }
            buf[at] = off as u32;
            buf[at + 1] = len as u32;
        }
        Field::Record(fields) => {
            let Value::Table(t) = v else {
                return Err(format!("{path}: expected a table (record) but got {}", type_name(v)));
            };
            let mut o = at;
            for (name, ff) in fields {
                let fv: Value = t
                    .raw_get(name.as_str())
                    .map_err(|e| format!("{path}.{name}: {e}"))?;
                write(ff, &fv, buf, o, total, &format!("{path}.{name}"))?;
                o += ff.words() as usize;
            }
        }
        Field::Records(slots) => {
            let count = (total as usize)
                .checked_mul(*slots as usize)
                .ok_or_else(|| format!("{path}: too many recorded values"))?;
            let off = buf.len();
            buf.resize(off + count, 0);
            if count > 0 {
                let Value::Table(t) = v else {
                    return Err(format!("{path}: missing recorded values"));
                };
                for i in 0..count {
                    let rv: Value = t.raw_get(i + 1).map_err(|e| format!("{path}: {e}"))?;
                    buf[off + i] = match rv {
                        Value::Nil => 0,
                        Value::Boolean(b) => b as u32,
                        ref x => match number(x) {
                            Some(n) => (n as f32).to_bits(),
                            None => {
                                return Err(format!(
                                    "{path}: a value recorded for the GPU must be a number or boolean, got {}",
                                    type_name(x)
                                ))
                            }
                        },
                    };
                }
            }
            buf[at] = off as u32;
        }
    }
    Ok(())
}
