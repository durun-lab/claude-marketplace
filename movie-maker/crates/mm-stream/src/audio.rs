//! 素材の音だけ。Story で文字起こしの所を聞くための、低いビットレートの m4a。

use std::fs;
use std::path::{Path, PathBuf};

use mm_doc::Project;
use mm_types::{Id, Result};

use crate::cache::{CacheDir, PARTIAL_EXT};
use crate::payload::{ranged_file, Payload, REVALIDATE};
use crate::quality::AUDIO_BITS_PER_SECOND;
use crate::tools::{args, path_arg, run_ffmpeg};

/// 素材の音だけの要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRequest {
    /// 素材（asset）の ID。
    pub asset: String,
}

/// 話し声が中心の素材なので、モノラルにして 64 kbps で足りる（1 時間で約 29 MB）。
const CHANNELS: &str = "1";

/// 素材の音を m4a にして置き、そのパスを返す。置いてあればそのまま返す。
pub(crate) fn ensure_audio(
    project: &Project,
    req: &AudioRequest,
    cache: &CacheDir,
) -> Result<PathBuf> {
    let asset = project.asset(&Id::new(req.asset.as_str()))?;
    let out = cache.audio(&asset.content_hash);
    if out.exists() {
        return Ok(out);
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut partial = out.as_os_str().to_owned();
    partial.push(".");
    partial.push(PARTIAL_EXT);
    let partial = PathBuf::from(partial);
    let mut a = args(["-i"]);
    a.push(path_arg(Path::new(&asset.path)));
    a.extend(args(["-vn", "-map", "0:a:0", "-c:a", "aac"]));
    a.extend(args(["-b:a".to_owned(), AUDIO_BITS_PER_SECOND.to_string()]));
    a.extend(args([
        "-ac",
        CHANNELS,
        "-movflags",
        "+faststart",
        "-f",
        "ipod",
    ]));
    a.push(path_arg(&partial));
    let result = run_ffmpeg(&a).and_then(|()| fs::rename(&partial, &out).map_err(Into::into));
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result.map(|()| out)
}

/// `path` の m4a を、`Range` ヘッダーに合わせて返す。長さの外を指す `Range` は 416。
pub(crate) fn respond(path: &Path, content_hash: &str, range: Option<&str>) -> Result<Payload> {
    let etag = format!("\"{content_hash}\"");
    ranged_file(path, "audio/mp4", (&etag, REVALIDATE), range)
}
