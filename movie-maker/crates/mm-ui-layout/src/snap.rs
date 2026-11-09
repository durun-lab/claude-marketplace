//! スナップ。ドラッグ中の時刻を、近い目標（再生位置、ほかのレイヤーとカットの端、キーフレーム、マーカー、
//! コンポジションの始まりと終わり）に吸い付ける。何に吸い付いたか（[`SnapTarget`]）は、スナップの表示と、
//! ステータスバーに出す文（[`SnapTarget::message`]）に使う。

use crate::gesture::Side;
use crate::timeline::TimelineModel;

/// 吸い付く距離（px）。
pub const SNAP_PX: f64 = 8.0;

/// スナップの目標の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// 再生位置。
    Playhead,
    /// レイヤーの帯の端。
    LayerEdge { item: usize, side: Side },
    /// シーケンスのカットの端。
    ClipEdge { item: usize, cut: usize, side: Side },
    /// レイヤーのキーフレーム。
    Keyframe { item: usize },
    /// コンポジションのマーカー。
    Marker,
    /// コンポジションの始まり（0 秒）。
    Start,
    /// コンポジションの終わり。
    End,
}

/// スナップの目標 1 つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapTarget {
    /// 時刻（秒）。
    pub time: f64,
    pub kind: SnapKind,
}

impl SnapTarget {
    fn new(time: f64, kind: SnapKind) -> Self {
        Self { time, kind }
    }

    /// 何に吸い付いたかの文。ステータスバーに出す。語は画面の文言（Playhead、Layer、Clip、Keyframe、Marker）にそろえる。
    pub fn message(&self) -> &'static str {
        match self.kind {
            SnapKind::Playhead => "Snapped to Playhead",
            SnapKind::LayerEdge { .. } => "Snapped to Layer Edge",
            SnapKind::ClipEdge { .. } => "Snapped to Clip Edge",
            SnapKind::Keyframe { .. } => "Snapped to Keyframe",
            SnapKind::Marker => "Snapped to Marker",
            SnapKind::Start => "Snapped to Start",
            SnapKind::End => "Snapped to End",
        }
    }
}

/// スナップの結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapped {
    /// 吸い付いた後の時刻。
    pub time: f64,
    /// 吸い付いた目標。どれにも近くなければ `None`。
    pub target: Option<SnapTarget>,
}

/// `time` を `targets` のうち `threshold_px` 以内で一番近いものへ寄せる。同じ近さなら並びの先のもの。
pub fn snap(time: f64, targets: &[SnapTarget], px_per_sec: f64, threshold_px: f64) -> Snapped {
    let limit = threshold_px / px_per_sec;
    let best = targets
        .iter()
        .filter(|t| (t.time - time).abs() <= limit)
        .min_by(|a, b| (a.time - time).abs().total_cmp(&(b.time - time).abs()))
        .copied();
    Snapped {
        time: best.map_or(time, |t| t.time),
        target: best,
    }
}

/// ドラッグで端を動かしているもの。その端は目標から外す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Moving {
    /// レイヤー `item` の帯の端（`trimLayer`）。そのレイヤーの端とカットの端を外す。
    Layer(usize),
    /// カットの端（`ripple`、`roll`）。そのカットの端だけを外す。
    Cut { item: usize, cut: usize },
}

/// `moving` の端を動かすときのスナップの目標。同じ近さのときに採る順（再生位置、レイヤーとカットの端、キーフレーム、
/// マーカー、始まりと終わり）に並べる。人が置いた印ほど先にし、始まりと終わりは端と重なりやすいので最後にする。
pub(crate) fn targets(model: &TimelineModel, moving: Moving, playhead: f64) -> Vec<SnapTarget> {
    let mut out = vec![SnapTarget::new(playhead, SnapKind::Playhead)];
    edges(model, moving, &mut out);
    for (item, it) in model.items.iter().enumerate() {
        let kind = SnapKind::Keyframe { item };
        out.extend(it.keyframes.iter().map(|&t| SnapTarget::new(t, kind)));
    }
    let markers = model.markers.iter();
    out.extend(markers.map(|&t| SnapTarget::new(t, SnapKind::Marker)));
    out.push(SnapTarget::new(0.0, SnapKind::Start));
    out.push(SnapTarget::new(model.duration, SnapKind::End));
    out
}

/// レイヤーとカットの端を `out` に足す。`moving` の端は除く。
fn edges(model: &TimelineModel, moving: Moving, out: &mut Vec<SnapTarget>) {
    for (item, it) in model.items.iter().enumerate() {
        if moving == Moving::Layer(item) {
            continue;
        }
        out.extend(sides(it.start, it.end, |side| SnapKind::LayerEdge {
            item,
            side,
        }));
        for (cut, bar) in it.cuts.iter().enumerate() {
            if moving != (Moving::Cut { item, cut }) {
                let kind = |side| SnapKind::ClipEdge { item, cut, side };
                out.extend(sides(bar.start, bar.end, kind));
            }
        }
    }
}

fn sides(start: f64, end: f64, kind: impl Fn(Side) -> SnapKind) -> [SnapTarget; 2] {
    [
        SnapTarget::new(start, kind(Side::In)),
        SnapTarget::new(end, kind(Side::Out)),
    ]
}

/// 時刻を `fps` のフレームの境目に丸める。
pub fn quantize(time: f64, fps: f64) -> f64 {
    if fps <= 0.0 {
        return time;
    }
    (time * fps).round() / fps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CutBar, Item, ItemKind};

    fn at(times: &[f64]) -> Vec<SnapTarget> {
        times
            .iter()
            .map(|&t| SnapTarget::new(t, SnapKind::Marker))
            .collect()
    }

    #[test]
    fn 近い目標へ吸い付く() {
        // 100 px/s で 8 px は 0.08 秒。
        let s = snap(2.05, &at(&[0.0, 2.0, 5.0]), 100.0, SNAP_PX);
        assert_eq!(s.time, 2.0);
        assert_eq!(s.target.map(|t| t.time), Some(2.0));
    }

    #[test]
    fn 遠ければそのまま() {
        let s = snap(3.5, &at(&[0.0, 2.0, 5.0]), 100.0, SNAP_PX);
        assert_eq!((s.time, s.target), (3.5, None));
    }

    #[test]
    fn 二つの目標の間では近い方を選ぶ() {
        let s = snap(1.04, &at(&[1.0, 1.06]), 100.0, SNAP_PX);
        assert_eq!(s.time, 1.06);
    }

    #[test]
    fn フレームの境目に丸める() {
        assert!((quantize(0.51, 30.0) - 15.0 / 30.0).abs() < 1e-12);
        assert_eq!(quantize(1.23, 0.0), 1.23);
    }

    /// シーケンス（0-10 秒、カット 0-3、3-4）と、キーフレームのある文字のレイヤー（2-4 秒）。マーカーは 6.5 秒。
    fn model() -> TimelineModel {
        let cut = |start, end| CutBar {
            start,
            end,
            ..CutBar::default()
        };
        let mut seq = Item::layer(0.0, 10.0, ItemKind::Sequence, true);
        seq.cuts = vec![cut(0.0, 3.0), cut(3.0, 4.0)];
        let mut text = Item::layer(2.0, 4.0, ItemKind::Text, true);
        text.keyframes = vec![2.5, 3.5];
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![seq, text],
            markers: vec![6.5],
        }
    }

    fn kinds_at(targets: &[SnapTarget], t: f64) -> Vec<SnapKind> {
        targets
            .iter()
            .filter(|s| s.time == t)
            .map(|s| s.kind)
            .collect()
    }

    #[test]
    fn レイヤーの端を動かすときは自分の端を除き_キーフレームとマーカーと再生位置を含む() {
        let t = targets(&model(), Moving::Layer(1), 7.0);
        assert_eq!(
            t[0],
            SnapTarget::new(7.0, SnapKind::Playhead),
            "再生位置が先"
        );
        let own = |s: &SnapTarget| matches!(s.kind, SnapKind::LayerEdge { item: 1, .. });
        assert!(!t.iter().any(own), "自分の端は除く");
        assert_eq!(kinds_at(&t, 2.5), [SnapKind::Keyframe { item: 1 }]);
        assert_eq!(kinds_at(&t, 6.5), [SnapKind::Marker]);
        assert_eq!(
            kinds_at(&t, 10.0),
            [
                SnapKind::LayerEdge {
                    item: 0,
                    side: Side::Out
                },
                SnapKind::End
            ],
            "同じ時刻なら端が先"
        );
    }

    #[test]
    fn カットの端を動かすときはそのカットの端だけを除く() {
        let t = targets(&model(), Moving::Cut { item: 0, cut: 0 }, 0.0);
        let clip = |cut, side| SnapKind::ClipEdge { item: 0, cut, side };
        assert_eq!(kinds_at(&t, 3.0), [clip(1, Side::In)], "自分の出は除く");
        assert_eq!(kinds_at(&t, 4.0)[0], clip(1, Side::Out));
        assert!(
            kinds_at(&t, 2.0).contains(&SnapKind::LayerEdge {
                item: 1,
                side: Side::In
            }),
            "ほかのレイヤーの端"
        );
    }

    #[test]
    fn キーフレームとマーカーに吸い付き_何に吸い付いたかの文を出す() {
        let t = targets(&model(), Moving::Layer(0), 9.0);
        let key = snap(3.47, &t, 100.0, SNAP_PX).target.expect("キーフレーム");
        assert_eq!(key.kind, SnapKind::Keyframe { item: 1 });
        assert_eq!(key.message(), "Snapped to Keyframe");
        let marker = snap(6.55, &t, 100.0, SNAP_PX).target.expect("マーカー");
        assert_eq!(marker.message(), "Snapped to Marker");
    }

    #[test]
    fn 同じ近さなら再生位置を採る() {
        let t = targets(&model(), Moving::Layer(0), 4.0);
        let s = snap(4.02, &t, 100.0, SNAP_PX).target.expect("4 秒");
        assert_eq!(s.message(), "Snapped to Playhead");
    }

    #[test]
    fn 文は目標の種類ごとに決まる() {
        let edge = SnapKind::LayerEdge {
            item: 0,
            side: Side::In,
        };
        let clip = SnapKind::ClipEdge {
            item: 0,
            cut: 1,
            side: Side::Out,
        };
        let cases = [
            (edge, "Snapped to Layer Edge"),
            (clip, "Snapped to Clip Edge"),
            (SnapKind::Start, "Snapped to Start"),
            (SnapKind::End, "Snapped to End"),
        ];
        for (kind, text) in cases {
            assert_eq!(SnapTarget::new(0.0, kind).message(), text);
        }
    }
}
