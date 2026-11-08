//! HLS の区間の割り方と、プレイリスト（m3u8）。

use mm_doc::Project;
use mm_types::{CompositionId, Id, MmError, Rational, RationalTime, Result, TimeRange};

use crate::key::SegmentKey;
use crate::quality::{stream_fps, HlsQuality};

/// 1 区間の長さの目標（秒）。fps の整数のフレーム数に切り上げる。
pub const SEGMENT_SECONDS: u32 = 4;

/// HLS の要求。コンポジションと画質で 1 本の再生になる（URL の `/m/api/stream/{comp}/{quality}.m3u8`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsRequest {
    /// 再生するコンポジションの ID。コメントの時刻と同じ時間軸になる。
    pub composition: String,
    pub quality: HlsQuality,
}

/// 1 本の再生の割り方。大きさ、fps、区間の数。
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub composition: CompositionId,
    pub quality: HlsQuality,
    pub fps: Rational,
    pub width: u32,
    pub height: u32,
    /// 1 区間のフレーム数（最後の区間以外）。
    pub segment_frames: i64,
    pub total_frames: i64,
}

impl Plan {
    pub(crate) fn new(project: &Project, req: &HlsRequest) -> Result<Self> {
        let composition: CompositionId = Id::new(req.composition.as_str());
        let comp = project.composition(&composition)?;
        let fps = stream_fps(comp.fps);
        let (width, height) = req.quality.size_for(comp.width, comp.height);
        let duration = comp.duration;
        let total_frames = ceil_div(
            duration.num() * i128::from(fps.num()),
            i128::from(duration.den()) * i128::from(fps.den()),
        );
        if total_frames <= 0 {
            return Err(MmError::InvalidArgument(format!(
                "コンポジション {} の長さが 0",
                req.composition
            )));
        }
        let segment_frames = ceil_div(
            i128::from(SEGMENT_SECONDS) * i128::from(fps.num()),
            i128::from(fps.den()),
        );
        Ok(Self {
            composition,
            quality: req.quality,
            fps,
            width,
            height,
            segment_frames: segment_frames as i64,
            total_frames: total_frames as i64,
        })
    }

    /// 区間の数。
    pub(crate) fn count(&self) -> u64 {
        ((self.total_frames + self.segment_frames - 1) / self.segment_frames) as u64
    }

    /// 区間 `index` のフレームの範囲（全体の通し番号、終わりを含まない）。
    fn frames(&self, index: u64) -> (i64, i64) {
        let start = index as i64 * self.segment_frames;
        (start, (start + self.segment_frames).min(self.total_frames))
    }

    /// 区間 `index` のコンポジションの時間軸での範囲。
    pub(crate) fn range(&self, index: u64) -> Result<TimeRange> {
        let (a, b) = self.frames(index);
        TimeRange::new(
            RationalTime::from_frames(a, self.fps)?,
            RationalTime::from_frames(b, self.fps)?,
        )
    }

    /// 区間 `index` の長さ（秒）。
    pub(crate) fn seconds(&self, index: u64) -> f64 {
        let (a, b) = self.frames(index);
        (b - a) as f64 / self.fps.to_f64()
    }

    /// VOD のプレイリスト。`keys` は区間の順のキーで、URI は `base` に区間のファイル名を足したもの。
    pub(crate) fn playlist(&self, keys: &[SegmentKey], base: &str) -> String {
        let longest = (0..self.count())
            .map(|i| self.seconds(i))
            .fold(0.0, f64::max);
        let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-PLAYLIST-TYPE:VOD\n");
        out.push_str(&format!(
            "#EXT-X-TARGETDURATION:{}\n",
            longest.round() as u64
        ));
        out.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");
        for (index, key) in keys.iter().enumerate() {
            out.push_str(&format!(
                "#EXTINF:{:.3},\n{base}{}\n",
                self.seconds(index as u64),
                key.file_name()
            ));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

/// 画質ごとのプレイリストを並べる親のプレイリスト。`BANDWIDTH` は区間のピーク、`AVERAGE-BANDWIDTH` は目標。
/// `variants` は画質の順の（計画、プレイリストの URI）。
pub(crate) fn master_playlist(variants: &[(Plan, String)]) -> String {
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
    for (plan, uri) in variants {
        out.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},AVERAGE-BANDWIDTH={},RESOLUTION={}x{},FRAME-RATE={:.3}\n{uri}\n",
            plan.quality.peak_bits_per_second(),
            plan.quality.total_bits_per_second(),
            plan.width,
            plan.height,
            plan.fps.to_f64(),
        ));
    }
    out
}

fn ceil_div(a: i128, b: i128) -> i128 {
    (a + b - 1).div_euclid(b)
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
            Rational::integer(30),
            RationalTime::seconds(seconds),
        ));
        p
    }

    fn req() -> HlsRequest {
        HlsRequest {
            composition: "main".into(),
            quality: HlsQuality::P360,
        }
    }

    #[test]
    fn 長さを区間に割り最後の区間は短い() {
        let plan = Plan::new(&project(10), &req()).unwrap();
        assert_eq!(plan.count(), 3);
        assert_eq!((plan.width, plan.height), (640, 360));
        assert!((plan.seconds(0) - 4.0).abs() < 1e-9);
        assert!((plan.seconds(2) - 2.0).abs() < 1e-9);
        let last = plan.range(2).unwrap();
        assert_eq!(last.end, RationalTime::seconds(10));
        assert_eq!(plan.range(1).unwrap().start, RationalTime::seconds(4));
    }

    #[test]
    fn プレイリストはキーの区間を全部並べて閉じる() {
        let plan = Plan::new(&project(10), &req()).unwrap();
        let keys: Vec<SegmentKey> = ["00000000000000a0", "00000000000000a1", "00000000000000a2"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        let text = plan.playlist(&keys, "seg/");
        assert!(text.starts_with("#EXTM3U\n"));
        assert!(text.contains("#EXT-X-TARGETDURATION:4\n"));
        assert!(text.contains("#EXTINF:4.000,\nseg/00000000000000a0.ts\n"));
        assert!(text.contains("#EXTINF:2.000,\nseg/00000000000000a2.ts\n"));
        assert!(text.ends_with("#EXT-X-ENDLIST\n"));
        assert_eq!(text.matches("#EXTINF").count(), 3);
    }

    #[test]
    fn 親のプレイリストは画質ごとにピークの_bandwidth_を載せる() {
        let variants: Vec<(Plan, String)> = HlsQuality::ALL
            .iter()
            .map(|&quality| {
                let req = HlsRequest { quality, ..req() };
                (
                    Plan::new(&project(10), &req).unwrap(),
                    format!("main/{}.m3u8", quality.as_str()),
                )
            })
            .collect();
        let text = master_playlist(&variants);
        let peak = HlsQuality::P360.peak_bits_per_second();
        assert!(text.contains(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={peak},AVERAGE-BANDWIDTH=500000,RESOLUTION=640x360,FRAME-RATE=30.000\nmain/360p.m3u8\n"
        )));
        assert!(text.contains("RESOLUTION=960x540"));
        assert_eq!(text.matches("#EXT-X-STREAM-INF").count(), 2);
    }

    #[test]
    fn 長さ_0_と無いコンポジションは拒む() {
        assert!(Plan::new(&project(0), &req()).is_err());
        let missing = HlsRequest {
            composition: "nope".into(),
            ..req()
        };
        assert!(Plan::new(&project(10), &missing).is_err());
    }
}
