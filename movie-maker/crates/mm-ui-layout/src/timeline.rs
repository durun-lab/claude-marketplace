//! Timeline のモデルと当たり判定。座標は Timeline の矩形の左上を (0, 0) とする。

use std::sync::Arc;

use crate::time_view::TimeView;
use crate::timeline_layout::Viewport;

/// 端のつまみの幅の上限。
pub const HANDLE_PX: f32 = 7.0;
/// 帯の最小の幅。短いレイヤーも掴めるようにする。
const MIN_BAR_PX: f32 = 3.0;

/// レイヤーの種類（色分けに使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Footage,
    Image,
    Solid,
    Text,
    Composition,
    /// カットを並べたシーケンス。`Item::cuts` に中身が入る。
    Sequence,
    Other,
}

/// シーケンスのカット 1 つ（有効なものだけ）。コンポジションの時間軸での始まりと終わり（秒）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CutBar {
    pub start: f64,
    pub end: f64,
    /// 帯に出す名前。空なら出さない。
    pub label: Arc<str>,
    /// 隙間（`lift` で抜いた所）。空きとして描く。
    pub gap: bool,
    /// シーケンスのカット列（無効なカットも含む）で、すぐ前のカットが無効。
    /// `roll` と `slide` は列の隣を動かすので、見えている隣と食い違うときは出さない。
    pub disabled_before: bool,
    /// シーケンスのカット列で、すぐ後ろのカットが無効。
    pub disabled_after: bool,
}

/// レイヤーの端のつまみで動かせる範囲（コンポジションの秒）。`trimLayer` が拒否しない範囲に合わせ、
/// ドラッグを素材の先頭と終わりで止める（Premiere や Resolve のトリムと同じ）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimBounds {
    /// 帯の左端を動かせる最も前の時刻（素材の先頭）。
    pub min_start: f64,
    /// 帯の右端を動かせる最も後の時刻（素材の終わり）。
    pub max_end: f64,
    /// イン点を動かせない（タイムリマップのあるレイヤー）。
    pub fixed_in: bool,
}

impl TrimBounds {
    /// 素材の長さが無いレイヤー（静止画、文字、単色など）。長さを 1 フレーム残す限り縛らない。
    pub const FREE: Self = Self {
        min_start: f64::NEG_INFINITY,
        max_end: f64::INFINITY,
        fixed_in: false,
    };
}

impl Default for TrimBounds {
    fn default() -> Self {
        Self::FREE
    }
}

/// 音のあるレイヤーの Mute と Solo。見出しに出す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AudioState {
    pub mute: bool,
    pub solo: bool,
}

/// Timeline に並ぶレイヤー 1 本。
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// レイヤーの ID。表示の状態（行の高さ）をレイヤーごとに引く。
    pub id: Arc<str>,
    /// コンポジションの時間軸での始まりと終わり（秒）。
    pub start: f64,
    pub end: f64,
    pub kind: ItemKind,
    pub enabled: bool,
    /// ロック（`lockLayer`）。Claude の操作経路から守る。
    pub locked: bool,
    /// 音のあるレイヤーの Mute と Solo。音の無いレイヤーは `None`。
    pub audio: Option<AudioState>,
    /// 帯に出す名前。空なら出さない。シーケンスのレイヤーはカットの名前を出す。
    pub label: Arc<str>,
    /// シーケンスのレイヤーのカット。先頭から順。シーケンス以外は空。
    pub cuts: Vec<CutBar>,
    /// 端のつまみで動かせる範囲。
    pub trim: TrimBounds,
}

impl Item {
    /// カットを持たない普通のレイヤー。
    pub fn layer(start: f64, end: f64, kind: ItemKind, enabled: bool) -> Self {
        Self {
            id: Arc::default(),
            start,
            end,
            kind,
            enabled,
            locked: false,
            audio: None,
            label: Arc::default(),
            cuts: Vec::new(),
            trim: TrimBounds::FREE,
        }
    }

    /// 帯に出す名前を付ける。
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.into();
        self
    }

    /// ロックの状態を付ける。
    pub fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// 音のあるレイヤーにする。
    pub fn with_audio(mut self, mute: bool, solo: bool) -> Self {
        self.audio = Some(AudioState { mute, solo });
        self
    }

    /// レイヤーの ID を付ける。
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.into();
        self
    }
}

/// Timeline に出すもの。`items` は上のレイヤーが先頭。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimelineModel {
    pub duration: f64,
    pub fps: f64,
    pub items: Vec<Item>,
}

impl TimelineModel {
    /// スナップの目標（0、終わり、各レイヤーとカットの端）。`skip` のレイヤーは除く。
    pub fn snap_targets(&self, skip: Option<usize>) -> Vec<f64> {
        self.targets_without(skip, None)
    }

    /// カット `(item, cut)` の端を動かすときのスナップの目標。そのカット自身の端は除く。
    pub fn cut_snap_targets(&self, item: usize, cut: usize) -> Vec<f64> {
        self.targets_without(None, Some((item, cut)))
    }

    fn targets_without(&self, skip: Option<usize>, skip_cut: Option<(usize, usize)>) -> Vec<f64> {
        let mut out = vec![0.0, self.duration];
        for (i, it) in self.items.iter().enumerate() {
            if Some(i) != skip {
                out.extend([it.start, it.end]);
            }
            if Some(i) == skip {
                continue;
            }
            for (c, bar) in it.cuts.iter().enumerate() {
                if Some((i, c)) != skip_cut {
                    out.extend([bar.start, bar.end]);
                }
            }
        }
        out
    }
}

/// 帯の上の位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Body,
    InHandle,
    OutHandle,
}

/// 当たった所。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// 見出しの列の上の角。
    Corner,
    /// レイヤーの見出し。`index` は行の添字。
    Header {
        index: usize,
    },
    Ruler,
    Layer {
        index: usize,
        part: Part,
    },
    /// シーケンスのカット。`item` はレイヤーの添字、`cut` はそのカットの添字。
    Cut {
        item: usize,
        cut: usize,
        part: Part,
    },
    /// 縦のスクロールバー。`thumb` はスクロールボックスの上か。
    Scrollbar {
        thumb: bool,
    },
    Empty,
}

/// 帯の左端と幅（x、w）。
pub fn bar_span(view: &TimeView, item: &Item) -> (f32, f32) {
    span(view, item.start, item.end)
}

/// カットの帯の左端と幅（x、w）。
pub fn cut_span(view: &TimeView, bar: &CutBar) -> (f32, f32) {
    span(view, bar.start, bar.end)
}

/// 時刻 `[start, end)` の帯の左端と幅（x、w）。短い帯も掴めるように最小の幅を持たせる。
pub(crate) fn span(view: &TimeView, start: f64, end: f64) -> (f32, f32) {
    let x = view.x_of(start);
    let w = (view.x_of(end) - x).max(MIN_BAR_PX);
    (x, w)
}

/// 幅 `bw`、左端 `bx` の帯の上の `x` が、どの部分か。つまみは帯の幅の 1/3 を超えない。
fn part_at(bx: f32, bw: f32, x: f32) -> Part {
    let handle = HANDLE_PX.min(bw / 3.0);
    if x < bx + handle {
        Part::InHandle
    } else if x >= bx + bw - handle {
        Part::OutHandle
    } else {
        Part::Body
    }
}

/// 点 (x, y)（Timeline の座標）に当たるもの。つまみは帯の幅の 1/3 を超えない。
pub fn hit_test(model: &TimelineModel, vp: &Viewport, x: f32, y: f32) -> Hit {
    let l = vp.layout;
    if l.ruler.contains(x, y) {
        return Hit::Ruler;
    }
    if l.corner.contains(x, y) {
        return Hit::Corner;
    }
    if l.headers.contains(x, y) {
        return vp
            .rows
            .at(y - l.headers.y + vp.scroll_y)
            .filter(|i| *i < model.items.len())
            .map_or(Hit::Empty, |index| Hit::Header { index });
    }
    if l.scrollbar.contains(x, y) {
        return Hit::Scrollbar {
            thumb: vp.thumb().contains(x, y),
        };
    }
    if !l.row_area.contains(x, y) {
        return Hit::Empty;
    }
    let Some((index, item)) = vp
        .rows
        .at(y - l.row_area.y + vp.scroll_y)
        .and_then(|i| Some((i, model.items.get(i)?)))
    else {
        return Hit::Empty;
    };
    let bar = vp.bar(index, item.start, item.end);
    if !bar.contains(x, y) {
        return Hit::Empty;
    }
    if let Some(hit) = cut_hit(item, vp, index, x) {
        return hit;
    }
    // カットの外に出ているレイヤーの端は、カットを持つレイヤーでもつまみになる。
    Hit::Layer {
        index,
        part: part_at(bar.x, bar.w, x),
    }
}

/// シーケンスのレイヤーの上の `x` に当たるカット。
fn cut_hit(item: &Item, vp: &Viewport, index: usize, x: f32) -> Option<Hit> {
    let x = x - vp.layout.row_area.x;
    item.cuts.iter().enumerate().find_map(|(cut, bar)| {
        let (cx, cw) = cut_span(&vp.time, bar);
        (x >= cx && x < cx + cw).then(|| Hit::Cut {
            item: index,
            cut,
            part: part_at(cx, cw, x),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline_layout::tests::TestViewport;
    use crate::timeline_layout::{HEADER_W, RULER_H};

    fn model() -> TimelineModel {
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![
                Item::layer(0.0, 10.0, ItemKind::Text, true),
                Item::layer(2.0, 4.0, ItemKind::Footage, true),
            ],
        }
    }

    /// 幅 1200、高さ 300 の Timeline。
    fn viewport(m: &TimelineModel) -> TestViewport {
        TestViewport::new(m, (1200.0, 300.0))
    }

    #[test]
    fn 目盛りの帯は_ruler() {
        let m = model();
        let f = viewport(&m);
        assert_eq!(hit_test(&m, &f.vp(), 300.0, 10.0), Hit::Ruler);
    }

    #[test]
    fn 帯の左右の端はつまみ_中は本体() {
        let m = model();
        let f = viewport(&m);
        let y = f.in_bar(1);
        let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
        assert_eq!(
            at(201.0),
            Hit::Layer {
                index: 1,
                part: Part::InHandle
            }
        );
        assert_eq!(
            at(300.0),
            Hit::Layer {
                index: 1,
                part: Part::Body
            }
        );
        assert_eq!(
            at(399.0),
            Hit::Layer {
                index: 1,
                part: Part::OutHandle
            }
        );
        assert_eq!(at(450.0), Hit::Empty);
    }

    #[test]
    fn 狭い帯ではつまみが幅の三分の一に収まる() {
        let mut m = model();
        m.items[1].end = 2.06; // 6 px
        let f = viewport(&m);
        let mid = hit_test(&m, &f.vp(), HEADER_W + 203.0, f.in_bar(1));
        assert!(matches!(
            mid,
            Hit::Layer {
                part: Part::Body,
                ..
            }
        ));
    }

    #[test]
    fn 帯と帯の隙間は何も無い() {
        let m = model();
        let f = viewport(&m);
        let (top, h) = f.rows.bar(0);
        let y = RULER_H + top + h + 1.0;
        assert_eq!(hit_test(&m, &f.vp(), 300.0, y), Hit::Empty);
    }

    #[test]
    fn スクロールした分だけ下の行に当たり_目盛りの帯の下に隠れた行には当たらない() {
        let items = (0..20)
            .map(|i| Item::layer(0.0, 10.0, ItemKind::Image, true).with_id(&i.to_string()))
            .collect();
        let m = TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items,
        };
        let mut f = viewport(&m);
        f.scroll_y = 28.0 * 5.0;
        let y = f.in_bar(7);
        assert_eq!(
            hit_test(&m, &f.vp(), 300.0, y),
            Hit::Layer {
                index: 7,
                part: Part::Body
            }
        );
        assert_eq!(hit_test(&m, &f.vp(), 300.0, RULER_H - 1.0), Hit::Ruler);
    }

    #[test]
    fn 縦のスクロールバーはスクロールボックスとそれ以外を分ける() {
        let items = vec![Item::layer(0.0, 10.0, ItemKind::Image, true); 50];
        let m = TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items,
        };
        let f = viewport(&m);
        let thumb = f.vp().thumb();
        let x = f.layout.scrollbar.x + 4.0;
        let hit = |y| hit_test(&m, &f.vp(), x, y);
        assert_eq!(hit(thumb.y + 2.0), Hit::Scrollbar { thumb: true });
        assert_eq!(hit(thumb.bottom() + 10.0), Hit::Scrollbar { thumb: false });
    }

    fn seq_model() -> TimelineModel {
        let cut = |start, end, gap| CutBar {
            start,
            end,
            gap,
            ..CutBar::default()
        };
        let mut seq = Item::layer(0.0, 10.0, ItemKind::Sequence, true);
        seq.cuts = vec![cut(0.0, 3.0, false), cut(3.0, 4.0, true)];
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![seq],
        }
    }

    #[test]
    fn シーケンスの上ではカットとその端に当たり_カットの外はレイヤーの本体() {
        let m = seq_model();
        let f = viewport(&m);
        let y = f.in_bar(0);
        let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
        let cut = |cut, part| Hit::Cut { item: 0, cut, part };
        assert_eq!(at(2.0), cut(0, Part::InHandle));
        assert_eq!(at(150.0), cut(0, Part::Body));
        assert_eq!(at(298.0), cut(0, Part::OutHandle));
        assert_eq!(at(350.0), cut(1, Part::Body), "隙間もカット");
        assert_eq!(
            at(700.0),
            Hit::Layer {
                index: 0,
                part: Part::Body
            }
        );
    }

    #[test]
    fn カットの外にあるシーケンスのレイヤーの端はレイヤーのつまみ() {
        let m = seq_model();
        let f = viewport(&m);
        let y = f.in_bar(0);
        let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
        let layer = |part| Hit::Layer { index: 0, part };
        assert_eq!(at(998.0), layer(Part::OutHandle), "カットは 4 秒まで");
        assert_eq!(at(700.0), layer(Part::Body));
        // 端がカットの中にあるときはカットのつまみが先。
        assert_eq!(
            at(2.0),
            Hit::Cut {
                item: 0,
                cut: 0,
                part: Part::InHandle
            }
        );
    }

    #[test]
    fn カットの端のスナップの目標は他のカットの端を含み_自分の端を除く() {
        let t = seq_model().cut_snap_targets(0, 0);
        assert!(t.contains(&4.0) && t.contains(&3.0), "次のカットの端");
        assert_eq!(t.iter().filter(|x| **x == 3.0).count(), 1, "自分の出は除く");
    }

    #[test]
    fn スナップの目標は自分の端を除く() {
        let t = model().snap_targets(Some(1));
        assert!(t.contains(&0.0) && t.contains(&10.0));
        assert!(!t.contains(&2.0) && !t.contains(&4.0));
    }
}
