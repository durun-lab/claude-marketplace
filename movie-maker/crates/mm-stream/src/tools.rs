//! 外部のツール（ffmpeg の CLI）を呼ぶ。HLS の最後の符号化と素材の音だけの変換が使う。

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Stdio};

use mm_types::{MmError, Result};

/// ffmpeg の場所を指す環境変数（`mm-ingest` と同じ名前）。
const ENV_FFMPEG: &str = "MM_FFMPEG";
/// エラーに載せる標準エラー出力の末尾の行数。
const STDERR_TAIL_LINES: usize = 3;

/// ffmpeg を `args` で走らせる。場所は `MM_FFMPEG`、無ければ PATH の `ffmpeg`。
pub(crate) fn run_ffmpeg(args: &[OsString]) -> Result<()> {
    let exe = std::env::var_os(ENV_FFMPEG).unwrap_or_else(|| "ffmpeg".into());
    run_with(&exe, args)
}

/// 見つからなければ `Unsupported`（入れ方を添える）、失敗したら標準エラーの末尾を添えて `Internal`。
fn run_with(exe: &OsStr, args: &[OsString]) -> Result<()> {
    let out = Command::new(exe)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(exe, &e))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut tail: Vec<&str> = stderr
        .trim()
        .lines()
        .rev()
        .take(STDERR_TAIL_LINES)
        .collect();
    tail.reverse();
    Err(MmError::Internal(format!(
        "ffmpeg が失敗した: {}",
        tail.join(" / ")
    )))
}

fn spawn_error(exe: &OsStr, e: &std::io::Error) -> MmError {
    let exe = Path::new(exe).display();
    if e.kind() == std::io::ErrorKind::NotFound {
        MmError::Unsupported(format!(
            "モバイルの配信: {exe} が見つからない（PATH に通すか {ENV_FFMPEG} で指す）"
        ))
    } else {
        MmError::Io(std::io::Error::new(e.kind(), format!("{exe}: {e}")))
    }
}

/// 引数の並びを作る。
pub(crate) fn args<I, S>(items: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    items.into_iter().map(Into::into).collect()
}

/// パスを引数にする。
pub(crate) fn path_arg(p: &Path) -> OsString {
    p.as_os_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_が無ければ入れ方を添えて_unsupported() {
        let err = run_with(OsStr::new("mm-stream-no-such-ffmpeg"), &[]).unwrap_err();
        let MmError::Unsupported(msg) = err else {
            panic!("Unsupported でない: {err:?}");
        };
        assert!(msg.contains("mm-stream-no-such-ffmpeg"), "{msg}");
        assert!(msg.contains("MM_FFMPEG"), "{msg}");
    }
}
