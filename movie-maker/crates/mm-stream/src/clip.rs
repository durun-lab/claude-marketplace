//! 提案の区間の短い動画（docs/design/12-mobile.md の「映像の届け方」）。
//!
//! 提案を適用したプロジェクトの、提案の範囲の前後に余白を足した所を、360p の H.264 と AAC の MP4 に書く。
//! 符号化は `mm-export` から `mm-encode-ffmpeg` の `h264`（CRF、moov を先頭に置く `+faststart`）で、
//! 端末の `<video>` が Range で読みながら再生を始められる。

use std::fs;
use std::path::Path;

use mm_doc::Project;
use mm_export::{export_project, ExportOptions};
use mm_runtime::Engine;
use mm_types::{CompositionId, Id, MmError, Rational, RationalTime, Result, TimeRange};

use crate::hls::frame_count;
use crate::quality::{stream_fps, HlsQuality};
use crate::segment::{with_target, TargetSpec};

/// 短い動画を書くコーデック（libx264 の CRF、AAC、MP4）。
const CLIP_CODEC: &str = "h264";
/// 短い動画の画質。HLS の軽い方と同じ大きさ。
const CLIP_QUALITY: HlsQuality = HlsQuality::P360;
/// 提案の範囲の前後に足す余白（秒）。前後のつながりも見えるようにする。
pub const CLIP_PAD_SECONDS: f64 = 2.0;
/// 短い動画の長さの上限（秒）。長い範囲は始まりからこの長さで切り、作る時間と量を抑える。
pub const CLIP_MAX_SECONDS: f64 = 30.0;

/// 提案の区間の短い動画の要求。`mm-serve` が提案の範囲（`comp_spans`）から作る。
#[derive(Debug, Clone, PartialEq)]
pub struct ClipRequest {
    /// 範囲を持つコンポジションの ID。
    pub composition: String,
    /// 提案の範囲の始まりと終わり（コンポジションの時間軸の秒）。余白はこの外に足す。
    pub start: f64,
    pub end: f64,
    /// 適用する操作の JSON。キーに入れ、同じ操作の同じ範囲は作り直さない。
    pub ops: String,
    /// 適用した操作が触る ID。描いた後に描画のキャッシュから捨て、保存したデータのフレームに混ぜない。
    pub touched: Vec<String>,
}

/// 短い動画の大きさ、fps、範囲。
#[derive(Debug, Clone)]
pub(crate) struct ClipPlan {
    pub composition: CompositionId,
    pub fps: Rational,
    pub width: u32,
    pub height: u32,
    /// コンポジションの時間軸の範囲。フレームの境に合わせる。
    pub range: TimeRange,
}

impl ClipPlan {
    /// 範囲の前後に余白を足し、コンポジションの長さと上限の長さに収める。収まる所が無ければ `InvalidArgument`。
    pub(crate) fn new(project: &Project, req: &ClipRequest) -> Result<Self> {
        if !(req.start.is_finite() && req.end.is_finite() && req.start <= req.end) {
            return Err(MmError::InvalidArgument(format!(
                "提案の範囲が読めない: {}〜{}",
                req.start, req.end
            )));
        }
        let composition: CompositionId = Id::new(req.composition.as_str());
        let comp = project.composition(&composition)?;
        let fps = stream_fps(comp.fps);
        let rate = fps.to_f64();
        let total = frame_count(comp.duration, fps);
        let first = (((req.start - CLIP_PAD_SECONDS) * rate).floor() as i64).max(0);
        let max_frames = (CLIP_MAX_SECONDS * rate).ceil() as i64;
        let last = (((req.end + CLIP_PAD_SECONDS) * rate).ceil() as i64)
            .min(total)
            .min(first + max_frames);
        if last <= first {
            return Err(MmError::InvalidArgument(format!(
                "提案の範囲 {}〜{} 秒はコンポジション {} の外",
                req.start, req.end, req.composition
            )));
        }
        let (width, height) = CLIP_QUALITY.size_for(comp.width, comp.height);
        let range = TimeRange::new(
            RationalTime::from_frames(first, fps)?,
            RationalTime::from_frames(last, fps)?,
        )?;
        Ok(Self {
            composition,
            fps,
            width,
            height,
            range,
        })
    }
}

/// 短い動画を描いて `out` に置く。一時ファイルは `mm-export` が `<キー>.partial.mp4` に置き、完成してから置き換える。
/// 描画のキャッシュに残った提案の適用後のフレームは、成否によらず捨てる。
pub(crate) fn render_clip(
    engine: &mut Engine,
    project: &Project,
    plan: &ClipPlan,
    out: &Path,
    touched: &[String],
) -> Result<()> {
    let span = tracing::info_span!("stream.clip", composition = %plan.composition);
    let _g = span.enter();
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let spec = TargetSpec {
        composition: &plan.composition,
        width: plan.width,
        height: plan.height,
        fps: plan.fps,
        codec: CLIP_CODEC,
    };
    let (project, target) = with_target(project, &spec, out);
    let options = ExportOptions {
        output: Some(out.to_path_buf()),
        target: Some(target),
        range: Some(plan.range),
        codec: Some(CLIP_CODEC.into()),
        ..ExportOptions::default()
    };
    let result = export_project(engine, &project, &options).map(|_| ());
    engine.invalidate_touched(touched);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_doc::Composition;

    fn project(seconds: i64) -> Project {
        let mut p = Project::new();
        p.compositions.push(Composition::new(
            Id::new("main"),
            1920,
            1080,
            Rational::integer(60),
            RationalTime::seconds(seconds),
        ));
        p
    }

    fn req(start: f64, end: f64) -> ClipRequest {
        ClipRequest {
            composition: "main".into(),
            start,
            end,
            ops: "[]".into(),
            touched: Vec::new(),
        }
    }

    fn secs(plan: &ClipPlan) -> (f64, f64) {
        (plan.range.start.to_f64(), plan.range.end.to_f64())
    }

    #[test]
    fn 範囲の前後に余白を足し_360p_と配信の_fps_にする() {
        let plan = ClipPlan::new(&project(20), &req(5.0, 7.5)).unwrap();
        assert_eq!(secs(&plan), (3.0, 9.5));
        assert_eq!((plan.width, plan.height), (640, 360));
        assert_eq!(plan.fps, Rational::integer(30));
    }

    #[test]
    fn 余白はコンポジションの端で切り_長い範囲は上限の長さにする() {
        assert_eq!(
            secs(&ClipPlan::new(&project(10), &req(0.5, 9.0)).unwrap()),
            (0.0, 10.0)
        );
        let long = ClipPlan::new(&project(120), &req(10.0, 100.0)).unwrap();
        assert_eq!(secs(&long), (8.0, 8.0 + CLIP_MAX_SECONDS));
    }

    #[test]
    fn 長さ_0_の範囲も余白で前後を見せ_外と読めない範囲は拒む() {
        assert_eq!(
            secs(&ClipPlan::new(&project(10), &req(4.0, 4.0)).unwrap()),
            (2.0, 6.0)
        );
        assert!(ClipPlan::new(&project(10), &req(13.0, 14.0)).is_err());
        assert!(ClipPlan::new(&project(10), &req(5.0, 4.0)).is_err());
        assert!(ClipPlan::new(&project(10), &req(f64::NAN, 4.0)).is_err());
        let missing = ClipRequest {
            composition: "nope".into(),
            ..req(1.0, 2.0)
        };
        assert!(ClipPlan::new(&project(10), &missing).is_err());
    }
}
