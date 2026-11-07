//! 外のコマンド（ffmpeg）を呼ぶ。HLS の最後の符号化と素材の音だけの変換が使う。

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use mm_types::{MmError, Result};

/// ffmpeg の場所を指す環境変数（`mm-ingest` と同じ名前）。
const ENV_FFMPEG: &str = "MM_FFMPEG";

/// ffmpeg を `args` で走らせる。失敗したら標準エラーの末尾を添えて `Internal`。
pub(crate) fn run_ffmpeg(args: &[OsString]) -> Result<()> {
    let exe = std::env::var_os(ENV_FFMPEG).unwrap_or_else(|| "ffmpeg".into());
    let out = Command::new(&exe)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .map_err(|e| MmError::NotFound(format!("ffmpeg を起動できない（{e}）")))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: String = stderr
        .trim()
        .lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .join(" / ");
    Err(MmError::Internal(format!("ffmpeg が失敗した: {tail}")))
}

/// 引数の並びを作る糖衣。
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
