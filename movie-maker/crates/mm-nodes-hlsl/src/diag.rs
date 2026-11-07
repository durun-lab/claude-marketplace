//! 変換の誤り。利用者の源の行と列を持ち、レイヤーの誤りとして `file:行:列: 本文` の形で出す。

use std::fmt;

/// DXC が誤りの行に付ける印。`fatal error:` も拾う。
const ERROR_MARK: &str = "error: ";

/// 変換の誤りの 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 源の名前（`#line` で付けた名前）。分からなければ `None`。
    pub file: Option<String>,
    /// 1 始まりの行。
    pub line: Option<u32>,
    /// 1 始まりの列。
    pub column: Option<u32>,
    /// 本文。
    pub message: String,
}

impl Diagnostic {
    /// 場所の分からない誤り。
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            file: None,
            line: None,
            column: None,
            message: message.into(),
        }
    }

    /// DXC の標準エラーから誤りの行を拾う。1 つも拾えなければ全文を 1 件にする。
    pub fn parse_dxc(stderr: &str) -> Vec<Self> {
        let found: Vec<Self> = stderr.lines().filter_map(parse_line).collect();
        if found.is_empty() {
            vec![Self::plain(stderr.trim())]
        } else {
            found
        }
    }
}

/// `<file>:<line>:<col>: error: <message>` の 1 行。Windows のパスはドライブの `:` を含むので、後ろから読む。
fn parse_line(line: &str) -> Option<Diagnostic> {
    let at = line.find(ERROR_MARK)?;
    let message = line[at + ERROR_MARK.len()..].trim().to_string();
    let head = line[..at].trim_end().trim_end_matches(':');
    let head = head
        .strip_suffix(" fatal")
        .unwrap_or(head)
        .trim_end_matches(':');
    let mut parts = head.rsplitn(3, ':');
    let column = parts.next().and_then(|s| s.trim().parse().ok());
    let row = parts.next().and_then(|s| s.trim().parse().ok());
    let file = parts.next().map(str::to_string);
    match (file, row, column) {
        (Some(file), Some(row), Some(column)) => Some(Diagnostic {
            file: Some(file),
            line: Some(row),
            column: Some(column),
            message,
        }),
        _ => Some(Diagnostic::plain(message)),
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line, self.column) {
            (Some(file), Some(line), Some(col)) => {
                write!(f, "{file}:{line}:{col}: {}", self.message)
            }
            (Some(file), Some(line), None) => write!(f, "{file}:{line}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 名前と行と列を拾う() {
        let d =
            Diagnostic::parse_dxc("code:3:12: error: use of undeclared identifier 'foo'\n  x\n");
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].to_string(),
            "code:3:12: use of undeclared identifier 'foo'"
        );
    }

    #[test]
    fn ドライブの付いたパスも後ろから読む() {
        let d = Diagnostic::parse_dxc(r"C:\t\shader.hlsl:7:1: fatal error: boom");
        assert_eq!(d[0].file.as_deref(), Some(r"C:\t\shader.hlsl"));
        assert_eq!((d[0].line, d[0].column), (Some(7), Some(1)));
        assert_eq!(d[0].message, "boom");
    }

    #[test]
    fn 形の違う出力は全文を_1_件にする() {
        let d = Diagnostic::parse_dxc("dxc failed : something\n");
        assert_eq!(d, vec![Diagnostic::plain("dxc failed : something")]);
    }
}
