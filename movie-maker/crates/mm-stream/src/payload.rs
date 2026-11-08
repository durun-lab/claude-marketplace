//! ハンドラーに返す応答の形。HTTP のクレートの型は使わず、状態・ヘッダー・本体だけを持つ。

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use mm_types::{MmError, Result};

/// キーが URL に入っていて、中身が変わらない応答の `Cache-Control`。
pub(crate) const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// 毎回確かめさせる応答の `Cache-Control`。
pub(crate) const REVALIDATE: &str = "no-cache";

/// ハンドラーがそのまま返せる応答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// HTTP の状態（200、206、416）。
    pub status: u16,
    /// 追加するヘッダー（`Content-Type`、`Cache-Control`、`ETag`、`Content-Range`、`Accept-Ranges`）。
    pub headers: Vec<(&'static str, String)>,
    /// 本体。
    pub body: Vec<u8>,
}

impl Payload {
    pub(crate) fn ok(content_type: &str, cache_control: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("Content-Type", content_type.to_owned()),
                ("Cache-Control", cache_control.to_owned()),
            ],
            body,
        }
    }

    pub(crate) fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }

    /// ヘッダーの値。名前は大文字小文字を区別しない。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// `path` のファイルを `Range` ヘッダーに合わせて返す（200、206、416）。毎回 `etag` で確かめさせる。
pub(crate) fn ranged_file(
    path: &Path,
    content_type: &str,
    etag: &str,
    range: Option<&str>,
) -> Result<Payload> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let base = |status: u16, body: Vec<u8>| Payload {
        status,
        ..Payload::ok(content_type, REVALIDATE, body)
            .with_header("Accept-Ranges", "bytes".into())
            .with_header("ETag", etag.to_owned())
    };
    match ByteRange::parse(range, len) {
        Ok(None) => {
            let mut body = Vec::with_capacity(len as usize);
            file.read_to_end(&mut body)?;
            Ok(base(200, body))
        }
        Ok(Some(r)) => {
            file.seek(SeekFrom::Start(r.start))?;
            let mut body = vec![0u8; (r.end - r.start + 1) as usize];
            file.read_exact(&mut body)?;
            Ok(base(206, body).with_header(
                "Content-Range",
                format!("bytes {}-{}/{len}", r.start, r.end),
            ))
        }
        Err(MmError::InvalidArgument(_)) => {
            Ok(base(416, Vec::new()).with_header("Content-Range", format!("bytes */{len}")))
        }
        Err(e) => Err(e),
    }
}

/// `Range: bytes=...` の 1 区間。両端を含む。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    /// `Range` ヘッダーの値を、長さ `len` の本体の範囲として読む。
    /// 無い・`bytes` 以外・複数区間は全体（`None`）。長さの外だけを指すときは `Err`（416）。
    pub fn parse(header: Option<&str>, len: u64) -> Result<Option<Self>> {
        let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
            return Ok(None);
        };
        if spec.contains(',') {
            return Ok(None);
        }
        let Some((from, to)) = spec.split_once('-') else {
            return Ok(None);
        };
        let unsatisfiable = || MmError::InvalidArgument(format!("範囲が本体の外: {spec}"));
        let last = len.checked_sub(1).ok_or_else(unsatisfiable)?;
        let (start, end) = if from.trim().is_empty() {
            // `-N` は末尾の N バイト。
            let n: u64 = to.trim().parse().map_err(|_| unsatisfiable())?;
            if n == 0 {
                return Err(unsatisfiable());
            }
            (len.saturating_sub(n), last)
        } else {
            let start: u64 = from.trim().parse().map_err(|_| unsatisfiable())?;
            let end = match to.trim() {
                "" => last,
                t => t.parse::<u64>().map_err(|_| unsatisfiable())?.min(last),
            };
            (start, end)
        };
        if start > end {
            return Err(unsatisfiable());
        }
        Ok(Some(Self { start, end }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_は開いた端と末尾の指定を読む() {
        let r = |h| ByteRange::parse(Some(h), 100).unwrap();
        assert_eq!(r("bytes=0-9"), Some(ByteRange { start: 0, end: 9 }));
        assert_eq!(r("bytes=90-"), Some(ByteRange { start: 90, end: 99 }));
        assert_eq!(r("bytes=-10"), Some(ByteRange { start: 90, end: 99 }));
        assert_eq!(r("bytes=50-500"), Some(ByteRange { start: 50, end: 99 }));
        assert_eq!(r("bytes=0-1,5-6"), None);
        assert_eq!(ByteRange::parse(None, 100).unwrap(), None);
    }

    #[test]
    fn 本体の外だけを指す_range_は拒む() {
        assert!(ByteRange::parse(Some("bytes=100-"), 100).is_err());
        assert!(ByteRange::parse(Some("bytes=5-2"), 100).is_err());
        assert!(ByteRange::parse(Some("bytes=0-1"), 0).is_err());
    }
}
