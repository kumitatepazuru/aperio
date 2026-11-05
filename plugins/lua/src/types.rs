//! FunctionSpec / 外部値で使う型の記法。
//!
//! 文法:
//! ```text
//! type   := scalar | type "[" N? "]" | "{" (name ":" type ("," name ":" type)*)? "}"
//! scalar := "float" | "number" | "int" | "bool" | "string" | "any"
//! ```
//! `number` は `float` の別名。`T[N]` は固定長、`T[]` は可変長の配列(Lua の 1 始まり配列)。

use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeSpec {
    Float,
    Int,
    Bool,
    String,
    /// CPU 専用。GPU に渡ると型エラー。
    Any,
    /// `len` が `None` なら可変長。
    Array(Box<TypeSpec>, Option<usize>),
    Record(IndexMap<String, TypeSpec>),
}

impl TypeSpec {
    /// GPU 側(Slang)で表現できる型か。string / any を含むものは CPU 専用。
    pub fn is_gpu_representable(&self) -> bool {
        match self {
            TypeSpec::Float | TypeSpec::Int | TypeSpec::Bool => true,
            TypeSpec::String | TypeSpec::Any => false,
            TypeSpec::Array(elem, _) => elem.is_gpu_representable(),
            TypeSpec::Record(fields) => fields.values().all(TypeSpec::is_gpu_representable),
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, TypeSpec::Float | TypeSpec::Int | TypeSpec::Bool)
    }
}

impl fmt::Display for TypeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeSpec::Float => f.write_str("float"),
            TypeSpec::Int => f.write_str("int"),
            TypeSpec::Bool => f.write_str("bool"),
            TypeSpec::String => f.write_str("string"),
            TypeSpec::Any => f.write_str("any"),
            TypeSpec::Array(elem, Some(n)) => write!(f, "{elem}[{n}]"),
            TypeSpec::Array(elem, None) => write!(f, "{elem}[]"),
            TypeSpec::Record(fields) => {
                f.write_str("{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str("}")
            }
        }
    }
}

/// 型記法のパースエラー。
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid type `{input}`: {reason}")]
pub struct TypeParseError {
    pub input: String,
    pub reason: String,
}

impl FromStr for TypeSpec {
    type Err = TypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut p = Parser {
            src: s,
            pos: 0,
        };
        let err = |reason: String| TypeParseError {
            input: s.to_string(),
            reason,
        };
        let ty = p.parse_type().map_err(err)?;
        p.skip_ws();
        if p.pos != s.len() {
            return Err(err(format!("unexpected `{}`", &s[p.pos..])));
        }
        Ok(ty)
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<&str, String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '_' || c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            return Err(match self.peek() {
                Some(c) => format!("expected a name, found `{c}`"),
                None => "expected a name, found end of input".to_string(),
            });
        }
        Ok(&self.src[start..self.pos])
    }

    fn parse_type(&mut self) -> Result<TypeSpec, String> {
        let mut ty = if self.eat('{') {
            let mut fields = IndexMap::new();
            if !self.eat('}') {
                loop {
                    let name = self.ident()?.to_string();
                    if !self.eat(':') {
                        return Err(format!("expected `:` after field `{name}`"));
                    }
                    let fty = self.parse_type()?;
                    if fields.insert(name.clone(), fty).is_some() {
                        return Err(format!("duplicate field `{name}`"));
                    }
                    if self.eat('}') {
                        break;
                    }
                    if !self.eat(',') {
                        return Err("expected `,` or `}` in record type".to_string());
                    }
                }
            }
            TypeSpec::Record(fields)
        } else {
            match self.ident()? {
                "float" | "number" => TypeSpec::Float,
                "int" => TypeSpec::Int,
                "bool" | "boolean" => TypeSpec::Bool,
                "string" => TypeSpec::String,
                "any" => TypeSpec::Any,
                other => return Err(format!("unknown type `{other}`")),
            }
        };
        // 後置の配列指定(`T[N]` / `T[]`)は何段でも付けられる。
        while self.eat('[') {
            self.skip_ws();
            let start = self.pos;
            while matches!(self.peek(), Some('0'..='9')) {
                self.pos += 1;
            }
            let len = if start == self.pos {
                None
            } else {
                let n: usize = self.src[start..self.pos]
                    .parse()
                    .map_err(|_| "array length is too large".to_string())?;
                if n == 0 {
                    return Err("array length must be at least 1".to_string());
                }
                Some(n)
            };
            if !self.eat(']') {
                return Err("expected `]`".to_string());
            }
            ty = TypeSpec::Array(Box::new(ty), len);
        }
        Ok(ty)
    }
}
