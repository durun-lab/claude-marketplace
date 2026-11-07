//! HLS の区間を 1 つ作る。レンダラーで描いて既存のエンコーダーで書き（`mm-export`）、配信のビットレートに
//! 符号化し直して MPEG-TS にする。
//!
//! 符号化を 2 回するのは、`mm-encode-ffmpeg` が品質一定（crf）か画素あたりのビット数でしか書けず、目標の
//! ビットレートと MPEG-TS の出力を持たないため。360p の 4 秒の中間は crf 18 で、画質の損はほぼ無い。
//! 1 区間ずつ別に作るので、MPEG-TS の時刻は区間の始まりの秒から始め、プレーヤーの時間軸を途切れさせない。
//! 区間ごとに PAT、PMT、SPS、PPS を持つ MPEG-TS なら、別の回に作った区間を並べ替えても 1 本として再生できる。

use std::fs;
use std::path::{Path, PathBuf};

use mm_doc::{Project, RenderTarget};
use mm_export::{export_project, ExportOptions};
use mm_runtime::Engine;
use mm_types::{Id, Rational, RenderTargetId, Result};

use crate::cache::PARTIAL_EXT;
use crate::hls::Plan;
use crate::quality::AUDIO_BITS_PER_SECOND;
use crate::tools::{args, path_arg, run_ffmpeg};

/// 中間の出力に使うレンダーターゲットの ID。
const TARGET_ID: &str = "mm-stream";
/// 中間の符号化。小さい画面の大きさなので、画質を落とさない設定にして最後の符号化に渡す。
const INTERMEDIATE_CODEC: &str = "h264";
/// 最後の符号化の x264 の preset。待たせないために速い側に置く。
const FINAL_PRESET: &str = "veryfast";
/// 映像の maxrate と bufsize の、目標に対する倍率（百分率）。区間が短いので、上振れを小さく抑える。
const MAXRATE_PERCENT: u64 = 115;
const BUFSIZE_PERCENT: u64 = 200;
/// 音のサンプリング周波数。
const AUDIO_SAMPLE_RATE: &str = "48000";

/// 区間 `index` を作って `out` に置く。書きかけは同じ場所の `.partial` に置き、完成してから名前を替える。
pub(crate) fn render_segment(
    engine: &mut Engine,
    project: &Project,
    plan: &Plan,
    index: u64,
    out: &Path,
) -> Result<()> {
    let span = tracing::info_span!("stream.segment", composition = %plan.composition, index);
    let _g = span.enter();
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let range = plan.range(index)?;
    let (project, target) = with_stream_target(project, plan);
    let mid = sibling(out, "mid.mp4");
    let partial = sibling(out, PARTIAL_EXT);
    let options = ExportOptions {
        output: Some(mid.clone()),
        target: Some(target),
        range: Some(range),
        codec: Some(INTERMEDIATE_CODEC.into()),
        ..ExportOptions::default()
    };
    let result = export_project(engine, &project, &options)
        .and_then(|_| to_transport_stream(&mid, &partial, range.start.to_f64(), plan))
        .and_then(|()| fs::rename(&partial, out).map_err(Into::into));
    let _ = fs::remove_file(&mid);
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// 配信の大きさと fps のレンダーターゲットを足したプロジェクトのデータ。
fn with_stream_target(project: &Project, plan: &Plan) -> (Project, RenderTargetId) {
    let id: RenderTargetId = Id::new(TARGET_ID);
    let mut project = project.clone();
    project.render_targets.retain(|t| t.id != id);
    project.render_targets.push(RenderTarget {
        id: id.clone(),
        composition: plan.composition.clone(),
        width: plan.width,
        height: plan.height,
        fps: plan.fps,
        codec: INTERMEDIATE_CODEC.into(),
        output_path: "mm-stream.mp4".into(),
        loudness_lufs: None,
        delivery: Default::default(),
    });
    (project, id)
}

/// `out` と同じ場所の、拡張子を足したパス（`<キー>.ts` → `<キー>.ts.mid.mp4`）。
fn sibling(out: &Path, ext: &str) -> PathBuf {
    let mut name = out
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".");
    name.push(ext);
    out.with_file_name(name)
}

/// 中間の MP4 を、目標のビットレートの H.264 と AAC の MPEG-TS にする。
fn to_transport_stream(mid: &Path, out: &Path, start_seconds: f64, plan: &Plan) -> Result<()> {
    let video = u64::from(plan.quality.video_bits_per_second());
    let fps: Rational = plan.fps;
    let mut a = args(["-i"]);
    a.push(path_arg(mid));
    a.extend(args(["-map", "0:v:0", "-map", "0:a:0?"]));
    a.extend(args([
        "-c:v",
        "libx264",
        "-preset",
        FINAL_PRESET,
        "-profile:v",
        "main",
    ]));
    a.extend(args(["-pix_fmt", "yuv420p"]));
    a.extend(args(["-b:v".to_owned(), video.to_string()]));
    a.extend(args([
        "-maxrate".to_owned(),
        (video * MAXRATE_PERCENT / 100).to_string(),
    ]));
    a.extend(args([
        "-bufsize".to_owned(),
        (video * BUFSIZE_PERCENT / 100).to_string(),
    ]));
    // 区間の頭だけがキーフレームになるようにし、シーンの切れ目の挿入を止める。
    a.extend(args(["-g".to_owned(), plan.segment_frames.to_string()]));
    a.extend(args(["-sc_threshold", "0"]));
    a.extend(args([
        "-r".to_owned(),
        format!("{}/{}", fps.num(), fps.den()),
    ]));
    a.extend(args(["-c:a", "aac"]));
    a.extend(args(["-b:a".to_owned(), AUDIO_BITS_PER_SECOND.to_string()]));
    a.extend(args(["-ar", AUDIO_SAMPLE_RATE]));
    a.extend(args([
        "-output_ts_offset".to_owned(),
        format!("{start_seconds:.6}"),
    ]));
    a.extend(args(["-muxdelay", "0", "-muxpreload", "0", "-f", "mpegts"]));
    a.push(path_arg(out));
    run_ffmpeg(&a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 補助のパスは元の名前に拡張子を足す() {
        let p = sibling(Path::new("a/b/k.ts"), "partial");
        assert_eq!(p, Path::new("a/b/k.ts.partial"));
    }
}
