//! HLS の区間を 1 つ作る。レンダラーで描き、`mm-export` が `mm-encode-ffmpeg` の `h264_ts` で配信のビットレートの
//! H.264 と AAC の MPEG-TS に 1 回で書く。
//!
//! 1 区間ずつ別に作るので、MPEG-TS の時刻は区間の始まりの秒から始め、プレーヤーの時間軸を途切れさせない。
//! 区間ごとに PAT、PMT、SPS、PPS を持つ MPEG-TS なら、別の回に作った区間を並べ替えても 1 本として再生できる。

use std::fs;
use std::path::Path;

use mm_doc::{Project, RenderTarget};
use mm_export::{export_project, ExportOptions, StreamOutput};
use mm_runtime::Engine;
use mm_types::{Id, MmError, RenderTargetId, Result, TimeRange};

use crate::hls::Plan;
use crate::quality::AUDIO_BITS_PER_SECOND;

/// 書き出しに使うレンダーターゲットの ID。
const TARGET_ID: &str = "mm-stream";
/// 配信の区間を書くコーデック（H.264 + AAC の MPEG-TS）。
const STREAM_CODEC: &str = "h264_ts";

/// 区間 `index` を作って `out` に置く。一時ファイルは `mm-export` が同じ場所の `<キー>.partial.ts` に置き、完成してから置き換える。
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
    let options = ExportOptions {
        output: Some(out.to_path_buf()),
        target: Some(target),
        range: Some(range),
        codec: Some(STREAM_CODEC.into()),
        stream: Some(stream_output(plan, range)?),
        ..ExportOptions::default()
    };
    export_project(engine, &project, &options).map(|_| ())
}

/// 区間の書き方。キーフレームは区間の長さごと、時刻は区間の始まりの秒から。
fn stream_output(plan: &Plan, range: TimeRange) -> Result<StreamOutput> {
    let gop = u32::try_from(plan.segment_frames)
        .map_err(|_| MmError::Internal("区間のフレーム数が大きすぎる".into()))?;
    Ok(StreamOutput {
        video_bit_rate: plan.quality.video_bits_per_second(),
        video_max_rate: Some(plan.quality.video_max_bits_per_second()),
        audio_bit_rate: AUDIO_BITS_PER_SECOND,
        keyframe_interval_frames: gop,
        start_offset: range.start,
    })
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
        codec: STREAM_CODEC.into(),
        output_path: "mm-stream.ts".into(),
        loudness_lufs: None,
        delivery: Default::default(),
    });
    (project, id)
}
