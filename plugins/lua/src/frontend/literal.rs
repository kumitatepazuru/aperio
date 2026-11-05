//! 数値・文字列リテラルの解釈(LuaJIT 2.1 の字句規則)。

/// 数値リテラルの解釈結果。
#[derive(Clone, Debug, PartialEq)]
pub enum NumberLit {
    Number(f64),
    /// LuaJIT の FFI cdata リテラル(`1LL`, `0x10ULL`, `3i`)。値は CPU でしか扱えない。
    CData,
}

/// Lua の数値リテラルを解釈する。不正なら None。
pub fn parse_number(text: &str) -> Option<NumberLit> {
    let lower = text.to_ascii_lowercase();
    if lower.ends_with("ll") || lower.ends_with('i') {
        return Some(NumberLit::CData);
    }
    if let Some(hex) = lower.strip_prefix("0x") {
        return parse_hex(hex).map(NumberLit::Number);
    }
    if let Some(bin) = lower.strip_prefix("0b") {
        // LuaJIT 2.1 は 0b 表記を受け付ける。
        let mut v = 0f64;
        for c in bin.chars() {
            v = v * 2.0
                + match c {
                    '0' => 0.0,
                    '1' => 1.0,
                    _ => return None,
                };
        }
        return Some(NumberLit::Number(v));
    }
    lower.parse::<f64>().ok().map(NumberLit::Number)
}

/// `0x` の後ろ(仮数部の 16 進 + 任意の小数部 + 任意の `p` 指数)を解釈する。
fn parse_hex(s: &str) -> Option<f64> {
    let (mantissa, exp) = match s.find('p') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    let mut v = 0f64;
    for c in int_part.chars() {
        v = v * 16.0 + c.to_digit(16)? as f64;
    }
    let mut scale = 1.0 / 16.0;
    for c in frac_part.chars() {
        v += c.to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    if let Some(e) = exp {
        let e: i32 = e.parse().ok()?;
        v *= 2f64.powi(e);
    }
    Some(v)
}

/// クォート付き文字列の中身(エスケープ処理前)をバイト列に変換する。
pub fn unescape(literal: &str) -> Result<Vec<u8>, String> {
    let bytes = literal.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&c) = bytes.get(i) else {
            return Err("unfinished escape sequence".to_string());
        };
        i += 1;
        match c {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'\'' => out.push(b'\''),
            b'\n' | b'\r' => {
                // `\` + 改行は改行 1 つ。\r\n / \n\r は 1 つとして扱う。
                out.push(b'\n');
                if let Some(&n) = bytes.get(i) {
                    if (n == b'\n' || n == b'\r') && n != c {
                        i += 1;
                    }
                }
            }
            b'x' => {
                let hex = bytes
                    .get(i..i + 2)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| "invalid \\x escape".to_string())?;
                out.push(hex);
                i += 2;
            }
            b'z' => {
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            b'u' => {
                if bytes.get(i) != Some(&b'{') {
                    return Err("missing `{` in \\u{XXXX}".to_string());
                }
                let close = literal[i..]
                    .find('}')
                    .ok_or_else(|| "missing `}` in \\u{XXXX}".to_string())?;
                let code = u32::from_str_radix(&literal[i + 1..i + close], 16)
                    .map_err(|_| "invalid \\u escape".to_string())?;
                let ch = char::from_u32(code).ok_or_else(|| "invalid \\u escape".to_string())?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += close + 1;
            }
            b'0'..=b'9' => {
                let mut v = (c - b'0') as u32;
                for _ in 0..2 {
                    match bytes.get(i) {
                        Some(d @ b'0'..=b'9') => {
                            v = v * 10 + (d - b'0') as u32;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                if v > 255 {
                    return Err("decimal escape too large".to_string());
                }
                out.push(v as u8);
            }
            other => {
                return Err(format!("invalid escape sequence `\\{}`", other as char));
            }
        }
    }
    Ok(out)
}

/// 長括弧文字列(`[[...]]`)の中身。先頭の改行 1 つは捨てる(Lua の仕様)。
pub fn long_string(literal: &str) -> Vec<u8> {
    let s = literal
        .strip_prefix("\r\n")
        .or_else(|| literal.strip_prefix("\n\r"))
        .or_else(|| literal.strip_prefix('\n'))
        .or_else(|| literal.strip_prefix('\r'))
        .unwrap_or(literal);
    s.as_bytes().to_vec()
}
