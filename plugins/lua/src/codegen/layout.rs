//! カーネルの params のレイアウト。
//!
//! params は 32 bit ワードの実行時長配列(`StructuredBuffer<LuaGpuWord>`)。
//! - 固定領域: トップレベルのフィールドを順に詰める(スカラーは 1 ワード、配列は (offset, length) の
//!   2 ワード、レコードはフィールドを順に展開、記録ブロックは開始位置の 1 ワード)。
//! - 可変長領域: 配列の要素と記録の値を、固定領域の後ろに追記する。
//!
//! CPU 側のディスパッチ用ブロックは、トップレベルのフィールドの値を同じ順に並べた Lua の配列を
//! `__lg_dispatch(k, values)` に渡し、Rust がこのレイアウトに従ってバイト列に詰める。
//! レイアウトは CPU 用 Lua の 1 行目のヘッダーに埋め込む([`encode_header`] / [`decode_header`])。

use crate::kernel::types::{ArrLen, GpuTy};

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    F32,
    Bool,
    U32,
    /// 実行時長の配列(要素は Field)。
    Array(Box<Field>),
    Record(Vec<(String, Field)>),
    /// 記録ブロック(1 反復あたりのスロット数)。値は Lua の平坦な配列。
    Records(u32),
}

impl Field {
    pub fn from_ty(ty: &GpuTy) -> Option<Field> {
        Some(match ty {
            GpuTy::Float | GpuTy::Unknown => Field::F32,
            GpuTy::Bool => Field::Bool,
            GpuTy::Array(e, ArrLen::Param) => Field::Array(Box::new(Field::from_ty(e)?)),
            GpuTy::Record(fields) => Field::Record(
                fields
                    .iter()
                    .map(|(k, t)| Some((k.clone(), Field::from_ty(t)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            _ => return None,
        })
    }

    /// 固定領域(または配列の要素)で占めるワード数。
    pub fn words(&self) -> u32 {
        match self {
            Field::F32 | Field::Bool | Field::U32 | Field::Records(_) => 1,
            Field::Array(_) => 2,
            Field::Record(fields) => fields.iter().map(|(_, f)| f.words()).sum(),
        }
    }

    fn encode(&self, out: &mut String) {
        match self {
            Field::F32 => out.push('f'),
            Field::Bool => out.push('b'),
            Field::U32 => out.push('u'),
            Field::Array(e) => {
                out.push_str("a(");
                e.encode(out);
                out.push(')');
            }
            Field::Record(fields) => {
                out.push_str("r(");
                for (i, (k, f)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(';');
                    }
                    out.push_str(k);
                    out.push('=');
                    f.encode(out);
                }
                out.push(')');
            }
            Field::Records(n) => out.push_str(&format!("R{n}")),
        }
    }
}

/// 1 カーネル分のレイアウト。
#[derive(Clone, Debug, PartialEq)]
pub struct KernelLayout {
    pub fields: Vec<Field>,
}

impl KernelLayout {
    /// 各トップレベルのフィールドの固定領域でのワード位置。
    pub fn offsets(&self) -> Vec<u32> {
        let mut at = 0;
        self.fields
            .iter()
            .map(|f| {
                let o = at;
                at += f.words();
                o
            })
            .collect()
    }

    pub fn fixed_words(&self) -> u32 {
        self.fields.iter().map(Field::words).sum()
    }
}

pub const HEADER_PREFIX: &str = "--[[lua_gpu v1 ";

/// ヘッダーの内容。
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// 元のチャンク名(実行時エラーの表示に使う)。
    pub chunk: String,
    pub kernels: Vec<KernelLayout>,
}

/// レイアウトをヘッダー文字列にする(`--[[lua_gpu v1 @main.lua|k0=u,f,f,u,a(f)|k1=...]]`)。
pub fn encode_header(chunk: &str, kernels: &[KernelLayout]) -> String {
    let mut s = String::from(HEADER_PREFIX);
    // チャンク名は区切り文字とコメントの終わりを含まないようにする。
    let safe: String = chunk
        .chars()
        .map(|c| if matches!(c, '|' | ']' | '\n' | '\r') { '_' } else { c })
        .collect();
    s.push('@');
    s.push_str(&safe);
    for (i, k) in kernels.iter().enumerate() {
        s.push('|');
        s.push_str(&format!("k{i}="));
        for (j, f) in k.fields.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            f.encode(&mut s);
        }
    }
    s.push_str("]]");
    s
}

/// CPU 用 Lua の先頭のヘッダーを読む。
pub fn decode_header(lua: &str) -> Result<Header, String> {
    let rest = lua
        .strip_prefix(HEADER_PREFIX)
        .ok_or_else(|| "missing lua_gpu header (is this the CPU Lua produced by convert?)".to_string())?;
    let end = rest.find("]]").ok_or_else(|| "unterminated lua_gpu header".to_string())?;
    let body = &rest[..end];
    let mut parts = body.split('|');
    let chunk = parts
        .next()
        .and_then(|p| p.strip_prefix('@'))
        .ok_or_else(|| "malformed lua_gpu header (missing chunk name)".to_string())?
        .to_string();
    let mut kernels = Vec::new();
    for (i, part) in parts.enumerate() {
        let spec = part
            .strip_prefix(&format!("k{i}="))
            .ok_or_else(|| format!("malformed kernel layout `{part}`"))?;
        let mut p = P { s: spec, i: 0 };
        let mut fields = Vec::new();
        if !spec.is_empty() {
            loop {
                fields.push(p.field()?);
                if p.eat(',') {
                    continue;
                }
                break;
            }
        }
        if p.i != spec.len() {
            return Err(format!("trailing data in kernel layout `{part}`"));
        }
        kernels.push(KernelLayout { fields });
    }
    Ok(Header { chunk, kernels })
}

struct P<'a> {
    s: &'a str,
    i: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c as u8) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("expected `{c}` in layout at {}", self.i))
        }
    }
    fn field(&mut self) -> Result<Field, String> {
        match self.peek() {
            Some(b'f') => {
                self.i += 1;
                Ok(Field::F32)
            }
            Some(b'b') => {
                self.i += 1;
                Ok(Field::Bool)
            }
            Some(b'u') => {
                self.i += 1;
                Ok(Field::U32)
            }
            Some(b'a') => {
                self.i += 1;
                self.expect('(')?;
                let e = self.field()?;
                self.expect(')')?;
                Ok(Field::Array(Box::new(e)))
            }
            Some(b'r') => {
                self.i += 1;
                self.expect('(')?;
                let mut fields = Vec::new();
                if !self.eat(')') {
                    loop {
                        let start = self.i;
                        while self.peek().is_some_and(|c| c == b'_' || c.is_ascii_alphanumeric()) {
                            self.i += 1;
                        }
                        let name = self.s[start..self.i].to_string();
                        self.expect('=')?;
                        fields.push((name, self.field()?));
                        if self.eat(';') {
                            continue;
                        }
                        self.expect(')')?;
                        break;
                    }
                }
                Ok(Field::Record(fields))
            }
            Some(b'R') => {
                self.i += 1;
                let start = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                self.s[start..self.i]
                    .parse()
                    .map(Field::Records)
                    .map_err(|_| "bad record slot count".to_string())
            }
            _ => Err(format!("unexpected layout character at {}", self.i)),
        }
    }
}
