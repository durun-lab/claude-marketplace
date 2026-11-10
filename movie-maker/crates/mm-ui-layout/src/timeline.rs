//! Timeline のモデルと当たり判定。座標は Timeline の矩形の左上を (0, 0) とする。

use std::sync::Arc;

use crate::header;
use crate::keymarks;
use crate::time_view::TimeView;
use crate::timeline_layout::Viewport;
use crate::wave::WaveSource;
use crate::zoombar::{self, ZoomBarPart};

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
    /// 帯の先頭に出すサムネイルのキー（素材のコンテンツハッシュ）。素材の無いカットは `None`。
    pub thumb: Option<Arc<str>>,
    /// 帯に波形を出す元（音のある撮影素材のカット）。
    pub wave: Option<WaveSource>,
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

/// 音を持てるレイヤー（撮影素材、入れ子、sequence）の Mute と Solo。レイヤーの見出しに出す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AudioState {
    pub mute: bool,
    pub solo: bool,
}

/// Timeline に並ぶレイヤー 1 本。
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// レイヤーの ID。レイヤーの見出しに名前として出し、表示の状態（行の高さ）をレイヤーごとに引く。
    pub id: Arc<str>,
    /// コンポジションの時間軸での始まりと終わり（秒）。
    pub start: f64,
    pub end: f64,
    pub kind: ItemKind,
    pub enabled: bool,
    /// ロック（`lockLayer`）。Claude の操作経路から守る。
    pub locked: bool,
    /// Shy のフラグ（View > Shy Selected Layers）。表示の状態なのでプロジェクトのデータからは作らず、アプリが立てる。
    pub shy: bool,
    /// 音を持てるレイヤーの Mute と Solo。音を持てないレイヤーは `None`。
    pub audio: Option<AudioState>,
    /// 帯に出す名前。空なら出さない。シーケンスのレイヤーはカットの名前を出す。
    pub label: Arc<str>,
    /// レイヤーのパラメータのキーフレームの時刻（コンポジションの秒、前から順、重複なし）。再フレーミングは含めない。
    pub keyframes: Arc<[f64]>,
    /// シーケンスのレイヤーのカット。先頭から順。シーケンス以外は空。
    pub cuts: Vec<CutBar>,
    /// 帯の先頭に出すサムネイルのキー（素材のコンテンツハッシュ）。撮影素材と静止画のレイヤーだけ。
    pub thumb: Option<Arc<str>>,
    /// 端のつまみで動かせる範囲。
    pub trim: TrimBounds,
    /// 帯に波形を出す元（音のある撮影素材のレイヤー）。シーケンスはカットが持つ。
    pub wave: Option<WaveSource>,
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
            shy: false,
            audio: None,
            label: Arc::default(),
            keyframes: Arc::default(),
            thumb: None,
            cuts: Vec::new(),
            trim: TrimBounds::FREE,
            wave: None,
        }
    }

    /// 帯に出す名前を付ける。
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.into();
        self
    }

    /// キーフレームの時刻（前から順）を付ける。
    pub fn with_keyframes(mut self, times: &[f64]) -> Self {
        self.keyframes = times.into();
        self
    }

    /// キーフレームを持つか（見出しの ▸ が引く）。
    pub fn has_keyframes(&self) -> bool {
        !self.keyframes.is_empty()
    }

    /// サムネイルのキーを付ける。
    pub fn with_thumb(mut self, key: &str) -> Self {
        self.thumb = Some(key.into());
        self
    }

    /// ロックの状態を付ける。
    pub fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    /// 音を持てるレイヤーにし、Mute と Solo を付ける。
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
    /// コンポジションのマーカーの時刻（秒）。スナップの目標にする。
    pub markers: Vec<f64>,
    /// Shy で Timeline から外した行の数。見出しの列の上の角に出す。
    pub hidden: usize,
}

/// レイヤーの見出しの、押せるボタン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderButton {
    /// 有効（`setLayerEnabled`）。
    Enabled,
    /// ロック（`lockLayer`）。
    Lock,
    /// Mute（`setLayerAudio`）。
    Mute,
    /// Solo（`setLayerAudio`）。
    Solo,
}

impl TimelineModel {
    /// 波形の元の素材のコンテンツハッシュ（重複なし、初めに出てきた順）。波形の読み込みが、読むキャッシュを知るのに使う。
    pub fn wave_hashes(&self) -> Vec<Arc<str>> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for item in &self.items {
            let cuts = item.cuts.iter().filter_map(|c| c.wave.as_ref());
            for w in item.wave.iter().chain(cuts) {
                if seen.insert(w.content_hash.clone()) {
                    out.push(w.content_hash.clone());
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
    /// レイヤーの見出しのボタン。`index` は行の添字。
    HeaderButton {
        index: usize,
        button: HeaderButton,
    },
    Ruler,
    Layer {
        index: usize,
        part: Part,
    },
    /// 帯のマーカー。`item` はレイヤーの添字、`keyframe` は `Item::keyframes` の添字（まとめたマーカーは先頭）。
    BarMark {
        item: usize,
        keyframe: usize,
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
    /// ズームバー。
    ZoomBar {
        part: ZoomBarPart,
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
    if let Some(hit) = region_hit(model, vp, x, y) {
        return hit;
    }
    let l = vp.layout;
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
    // 帯のマーカーは端のつまみより上に描くので、重なる所ではマーカーが勝つ（つまみはマーカーの上で掴める）。
    if let Some(mark) = keymarks::mark_at(vp, index, item, (x, y)) {
        return Hit::BarMark {
            item: index,
            keyframe: mark.keyframe,
        };
    }
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

/// 行の領域のほかの矩形（目盛りの帯、見出しの列の上の角、見出しの列、縦のスクロールバー、ズームバー）に当たるもの。
/// どれにも入らなければ `None`。
fn region_hit(model: &TimelineModel, vp: &Viewport, x: f32, y: f32) -> Option<Hit> {
    let l = vp.layout;
    if l.ruler.contains(x, y) {
        return Some(Hit::Ruler);
    }
    if l.corner.contains(x, y) {
        return Some(Hit::Corner);
    }
    if l.headers.contains(x, y) {
        let row = vp.rows.at(y - l.headers.y + vp.scroll_y);
        let item = row.and_then(|index| Some((index, model.items.get(index)?)));
        return Some(item.map_or(Hit::Empty, |(index, item)| {
            header::hit(vp, (index, item), (x, y))
        }));
    }
    if l.scrollbar.contains(x, y) {
        let thumb = vp.thumb().contains(x, y);
        return Some(Hit::Scrollbar { thumb });
    }
    if l.zoombar.contains(x, y) {
        let part = zoombar::part_at(zoombar::thumb_of(vp, model.duration), x);
        return Some(Hit::ZoomBar { part });
    }
    None
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
mod tests;
