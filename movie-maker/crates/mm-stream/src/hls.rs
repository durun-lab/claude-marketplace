//! HLS の区間の割り方と、プレイリスト（m3u8）。

use std::fmt;
use std::str::FromStr;

use mm_doc::Project;
use mm_types::{CompositionId, Id, MmError, Rational, RationalTime, Result, TimeRange};

use crate::quality::{stream_fps, HlsQuality};

/// 1 区間の長さの目標（秒）。fps の整数のフレーム数に切り上げる。
pub const SEGMENT_SECONDS: u32 = 4;
/// 描き元のレンダーターゲットが無いときの大きさと fps。
const FALLBACK_SIZE: (u32, u32) = (1280, 720);
const FALLBACK_FPS: u32 = 30;

/// HLS の要求。列（sequence）と画質で 1 本の再生になる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsRequest {
    /// 再生する sequence の ID。
    pub sequence: String,
    pub quality: HlsQuality,
}

/// プレイリストが並べる区間のファイル名（`v<版>-<番号>.ts`）。
/// ハンドラーは URL の最後の部分をこれで読み、[`crate::Streamer::segment`] に渡す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentName {
    /// プロジェクトの版。プレイリストを作った時の版で、編集で変わると古い URL は読めなくなる。
    pub version: u64,
    /// 区間の番号（0 から）。
    pub index: u64,
}

impl fmt::Display for SegmentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}-{}.ts", self.version, self.index)
    }
}

impl FromStr for SegmentName {
    type Err = MmError;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || MmError::InvalidArgument(format!("区間の名前として読めない: {s}"));
        let body = s.strip_prefix('v').and_then(|r| r.strip_suffix(".ts"));
        let (version, index) = body.and_then(|b| b.split_once('-')).ok_or_else(bad)?;
        Ok(Self {
            version: version.parse().map_err(|_| bad())?,
            index: index.parse().map_err(|_| bad())?,
        })
    }
}

/// 1 本の再生の割り方。大きさ、fps、区間の数。
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub sequence: String,
    pub quality: HlsQuality,
    pub fps: Rational,
    pub width: u32,
    pub height: u32,
    /// 描き元のコンポジション。見せ方（再フレーミング）と音の設定を引き継ぐ元。
    pub composition: CompositionId,
    /// 1 区間のフレーム数（最後の区間以外）。
    pub segment_frames: i64,
    pub total_frames: i64,
}

impl Plan {
    pub(crate) fn new(project: &Project, req: &HlsRequest) -> Result<Self> {
        let id = Id::new(req.sequence.as_str());
        let duration = mm_eval::sequence_duration(project.sequence(&id)?)?;
        let (src_w, src_h, src_fps, composition) = match project.render_targets.first() {
            Some(t) => (t.width, t.height, t.fps, t.composition.clone()),
            None => {
                let comp = project.compositions.first();
                (
                    comp.map_or(FALLBACK_SIZE.0, |c| c.width),
                    comp.map_or(FALLBACK_SIZE.1, |c| c.height),
                    comp.map_or(Rational::integer(FALLBACK_FPS), |c| c.fps),
                    comp.map_or_else(|| Id::new("main"), |c| c.id.clone()),
                )
            }
        };
        let fps = stream_fps(src_fps);
        let (width, height) = req.quality.size_for(src_w, src_h);
        let total_frames = ceil_div(
            duration.num() * i128::from(fps.num()),
            i128::from(duration.den()) * i128::from(fps.den()),
        );
        if total_frames <= 0 {
            return Err(MmError::InvalidArgument(format!(
                "sequence {} に有効なカットが無い",
                req.sequence
            )));
        }
        let segment_frames = ceil_div(
            i128::from(SEGMENT_SECONDS) * i128::from(fps.num()),
            i128::from(fps.den()),
        );
        Ok(Self {
            sequence: req.sequence.clone(),
            quality: req.quality,
            fps,
            width,
            height,
            composition,
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

    /// 区間 `index` の時間の範囲。
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

    /// VOD のプレイリスト。区間の URI は `base` に区間のファイル名を足したもの。
    pub(crate) fn playlist(&self, version: u64, base: &str) -> String {
        let count = self.count();
        let longest = (0..count).map(|i| self.seconds(i)).fold(0.0, f64::max);
        let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-PLAYLIST-TYPE:VOD\n");
        out.push_str(&format!(
            "#EXT-X-TARGETDURATION:{}\n",
            longest.round() as u64
        ));
        out.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");
        for index in 0..count {
            let name = SegmentName { version, index };
            out.push_str(&format!(
                "#EXTINF:{:.3},\n{base}{name}\n",
                self.seconds(index)
            ));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

fn ceil_div(a: i128, b: i128) -> i128 {
    (a + b - 1).div_euclid(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_doc::{Cut, Node, Sequence};

    fn project(seconds: i64) -> Project {
        let cut = Cut::new(
            "a",
            Node::new("solid", 1),
            RationalTime::ZERO,
            RationalTime::seconds(seconds),
        );
        let mut p = Project::new();
        p.sequences.push(Sequence {
            id: Id::new("s"),
            cuts: vec![cut],
            origin: None,
        });
        p
    }

    fn req() -> HlsRequest {
        HlsRequest {
            sequence: "s".into(),
            quality: HlsQuality::P360,
        }
    }

    #[test]
    fn 区間の名前は表示と読み取りで往復する() {
        let n = SegmentName {
            version: 12,
            index: 3,
        };
        assert_eq!(n.to_string(), "v12-3.ts");
        assert_eq!("v12-3.ts".parse::<SegmentName>().unwrap(), n);
        assert!("12-3.ts".parse::<SegmentName>().is_err());
        assert!("v1-x.ts".parse::<SegmentName>().is_err());
    }

    #[test]
    fn 長さを区間に割り最後の区間は短い() {
        let plan = Plan::new(&project(10), &req()).unwrap();
        assert_eq!(plan.count(), 3);
        assert!((plan.seconds(0) - 4.0).abs() < 1e-9);
        assert!((plan.seconds(2) - 2.0).abs() < 1e-9);
        let last = plan.range(2).unwrap();
        assert_eq!(last.end, RationalTime::seconds(10));
        assert_eq!(plan.range(1).unwrap().start, RationalTime::seconds(4));
    }

    #[test]
    fn プレイリストは版つきの区間を全部並べて閉じる() {
        let plan = Plan::new(&project(10), &req()).unwrap();
        let text = plan.playlist(7, "seg/");
        assert!(text.starts_with("#EXTM3U\n"));
        assert!(text.contains("#EXT-X-TARGETDURATION:4\n"));
        assert!(text.contains("#EXTINF:4.000,\nseg/v7-0.ts\n"));
        assert!(text.contains("#EXTINF:2.000,\nseg/v7-2.ts\n"));
        assert!(text.ends_with("#EXT-X-ENDLIST\n"));
        assert_eq!(text.matches("#EXTINF").count(), 3);
    }

    #[test]
    fn 空の_sequence_は拒む() {
        let mut p = project(10);
        p.sequences[0].cuts.clear();
        assert!(Plan::new(&p, &req()).is_err());
    }
}
