//! 変換時の診断メッセージ(エラー・警告)。

use std::fmt;

/// 診断の重大度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// 変換を失敗させる。
    Error,
    /// 変換は続行するが、意味のずれや性能上の懸念を知らせる。
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// ソース上の位置(1 始まり)。`chunk` はチャンク名(ファイル名やモジュール名)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub chunk: String,
    pub line: u32,
    pub col: u32,
}

impl Location {
    pub fn new(chunk: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            chunk: chunk.into(),
            line,
            col,
        }
    }
}

/// 1 件の診断。`code` は機械可読な識別子(例: `gpu-value-to-cpu`)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub location: Location,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {} [{}]: {}",
            self.location.chunk,
            self.location.line,
            self.location.col,
            self.severity.as_str(),
            self.code,
            self.message
        )
    }
}

/// 診断の集まり。変換の各段階がここに追記する。
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error(&mut self, code: &'static str, location: Location, message: impl Into<String>) {
        self.push(Severity::Error, code, location, message);
    }

    pub fn warning(&mut self, code: &'static str, location: Location, message: impl Into<String>) {
        self.push(Severity::Warning, code, location, message);
    }

    pub fn push(
        &mut self,
        severity: Severity,
        code: &'static str,
        location: Location,
        message: impl Into<String>,
    ) {
        self.items.push(Diagnostic {
            severity,
            code,
            message: message.into(),
            location,
        });
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    pub fn into_items(self) -> Vec<Diagnostic> {
        self.items
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter().filter(|d| d.severity == Severity::Warning)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.items {
            writeln!(f, "{d}")?;
        }
        Ok(())
    }
}
