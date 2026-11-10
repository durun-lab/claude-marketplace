//! Timeline の上端のツールバー。左から Undo と Redo、ツールの場所（Selection、Trim、Blade）、Split Clip、Lift、Extract、
//! Snapping、右に Zoom Out、スライダー、Zoom In、Full Extent Zoom を並べる（Resolve と CapCut の Timeline の上端と同じ）。
//! 座標は渡された矩形（Timeline の割り付けの `toolbar`）の左上が基準。
//!
//! ボタンの矩形は [`cells`] 1 か所で決め、描く関数（`paint/toolbar.rs`、[`toolbar_labels`]）、当たり判定
//! （[`ToolbarController`]）、`--play-test` のクリックの位置（[`toolbar_button`]）が同じ値を見る。
//! ボタンはコマンドの表（アプリの `commands.rs`）の ID を持ち、押して同じボタンの上で離すと、その ID を
//! [`ToolbarAction::Command`] で返す。押せるかはアプリがコマンドの表から作った [`ToolbarState::disabled`]、Snapping の
//! 押された見た目は [`ToolbarState::snapping`] で決める。名前は幅が足りなければ省き、アイコンだけにする。
//! スライダーは秒あたりの論理ピクセルを対数で動かす（[`zoom_fraction`]、[`zoom_px`]）。

mod controller;
mod icons;
mod labels;
mod look;

use crate::geom::Rect;
use crate::time_view::MAX_PX_PER_SEC;

pub use controller::ToolbarController;
pub use controller::{ToolbarAction, ToolbarPart};
pub(crate) use icons::{icon_quads, Icon};
pub use labels::toolbar_labels;
pub(crate) use look::ButtonLook;

/// ボタンの高さ。ツールバーの高さの中で縦に中央へ置く。
const BUTTON_H: f32 = 24.0;
/// ツールバーの左右の余白。
const PAD: f32 = 8.0;
/// ボタンの間隔とグループの間隔。
const BUTTON_GAP: f32 = 2.0;
const GROUP_GAP: f32 = 12.0;
/// アイコンだけのボタンの幅。
const ICON_BUTTON_W: f32 = 28.0;
/// 名前のあるボタンの、アイコンの欄の幅、アイコンと名前の間隔、左右の余白。
pub(crate) const ICON_W: f32 = 16.0;
const NAME_GAP: f32 = 4.0;
const NAME_PAD: f32 = 6.0;
/// 名前の幅の見積もり。字の大きさ 12 の英字 1 字の平均の幅（Tahoma で約 6）。
const NAME_CHAR_W: f32 = 6.0;
/// スライダーの幅（欲しい幅と、これより狭ければ出さない下限）とつまみの幅。
const SLIDER_W: f32 = 110.0;
const SLIDER_MIN_W: f32 = 56.0;
pub(crate) const THUMB_W: f32 = 10.0;

/// スナップのコマンドの ID。このボタンだけが入/切の見た目を持つ。
const SNAPPING: &str = "timeline.toggle_snapping";

/// ボタンの定義。`command` はコマンドの表の ID。ツールのボタンは Z4 までコマンドを持たず、場所だけを空けておく。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ButtonSpec {
    pub command: Option<&'static str>,
    pub icon: Icon,
    /// 幅が足りるときにアイコンの横に出す短い名前。
    pub name: &'static str,
    /// 左に広い間隔（グループの区切り）を空ける。
    pub gap_before: bool,
    /// 常に入の見た目にする（今のツール）。
    pub lit: bool,
}

const fn button(
    command: Option<&'static str>,
    icon: Icon,
    name: &'static str,
    gap_before: bool,
) -> ButtonSpec {
    ButtonSpec {
        command,
        icon,
        name,
        gap_before,
        lit: false,
    }
}

/// 左のグループ。ツールは Selection だけが今のツールで、Trim と Blade は Z4 まで押せない。
static LEFT: &[ButtonSpec] = &[
    button(Some("edit.undo"), Icon::Glyph('\u{E7A7}'), "Undo", false),
    button(Some("edit.redo"), Icon::Glyph('\u{E7A6}'), "Redo", false),
    ButtonSpec {
        lit: true,
        ..button(None, Icon::Selection, "Selection", true)
    },
    button(None, Icon::Trim, "Trim", false),
    button(None, Icon::Blade, "Blade", false),
    button(Some("clip.split"), Icon::Glyph('\u{E8C6}'), "Split", true),
    button(Some("clip.lift"), Icon::Glyph('\u{E74A}'), "Lift", false),
    button(
        Some("clip.extract"),
        Icon::Glyph('\u{E74D}'),
        "Extract",
        false,
    ),
    button(Some(SNAPPING), Icon::Snapping, "Snapping", true),
];

/// 右のズームのグループのボタン（Zoom Out と Zoom In は名前を持たず、Full Extent Zoom は「Fit」）。
static ZOOM_OUT: ButtonSpec = button(
    Some("timeline.zoom_out"),
    Icon::Glyph('\u{E71F}'),
    "",
    false,
);
static ZOOM_IN: ButtonSpec = button(Some("timeline.zoom_in"), Icon::Glyph('\u{E8A3}'), "", false);
static FIT: ButtonSpec = button(
    Some("timeline.full_extent_zoom"),
    Icon::Glyph('\u{E740}'),
    "Fit",
    false,
);

/// 描く関数の入力に渡す、ツールバーの状態とポインター操作の状態。
#[derive(Debug, Clone, Copy)]
pub struct ToolbarView<'a> {
    pub state: &'a ToolbarState,
    pub controller: &'a ToolbarController,
}

/// ツールバーが出す状態。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarState {
    /// 今は押せないコマンドの ID。
    pub disabled: Vec<&'static str>,
    /// スナップが入（Snapping を押された見た目にする）。
    pub snapping: bool,
    /// 今の秒あたりの論理ピクセルと、縮小の下限（`TimeView::zoom_floor`）。スライダーの位置と、ドラッグの換算に使う。
    pub px_per_sec: f64,
    pub floor: f64,
}

impl ToolbarState {
    /// ボタンが押せるか。コマンドの無いボタンは、今のツールだけが入で、残りは押せない。
    pub(crate) fn enabled(&self, spec: &ButtonSpec) -> bool {
        spec.command
            .map_or(spec.lit, |c| !self.disabled.contains(&c))
    }

    /// ボタンが入の見た目か。
    pub(crate) fn lit(&self, spec: &ButtonSpec) -> bool {
        spec.lit || (spec.command == Some(SNAPPING) && self.snapping)
    }
}

/// 1 つのボタンの矩形と定義。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ButtonCell {
    pub spec: &'static ButtonSpec,
    pub rect: Rect,
    /// アイコンの横に名前を出す。
    pub named: bool,
}

impl ButtonCell {
    /// アイコンの欄。名前があるときは左、無いときはボタンの中央。縦はボタンの高さ。
    pub(crate) fn icon_box(&self) -> Rect {
        let r = self.rect;
        let x = if self.named {
            r.x + NAME_PAD
        } else {
            r.x + (r.w - ICON_W) / 2.0
        };
        Rect::new(x, r.y, ICON_W, r.h)
    }

    /// 名前の欄。名前を出さないときは `None`。
    pub(crate) fn name_box(&self) -> Option<Rect> {
        let x = self.icon_box().right() + NAME_GAP;
        let w = self.rect.right() - NAME_PAD - x;
        (self.named && w > 0.0).then(|| Rect::new(x, self.rect.y, w, self.rect.h))
    }
}

/// スライダー。`rect` がクリックを受ける矩形、`track` が溝（つまみの中心が動く範囲）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slider {
    pub rect: Rect,
    pub track: Rect,
}

impl Slider {
    /// 溝の `x` の位置。
    pub(crate) fn fraction_at(&self, x: f32) -> f32 {
        if self.track.w <= 0.0 {
            return 0.0;
        }
        ((x - self.track.x) / self.track.w).clamp(0.0, 1.0)
    }

    /// 位置 `fraction` のつまみの中心の x。
    pub(crate) fn thumb_x(&self, fraction: f32) -> f32 {
        self.track.x + self.track.w * fraction
    }
}

/// ツールバーの部品の矩形。
#[derive(Debug, Clone)]
pub(crate) struct Cells {
    pub buttons: Vec<ButtonCell>,
    /// 幅が足りないときは `None`。
    pub slider: Option<Slider>,
    /// グループの間の縦線。
    pub separators: Vec<Rect>,
}

fn button_w(spec: &ButtonSpec, named: bool) -> f32 {
    if named && !spec.name.is_empty() {
        NAME_PAD * 2.0 + ICON_W + NAME_GAP + spec.name.chars().count() as f32 * NAME_CHAR_W
    } else {
        ICON_BUTTON_W
    }
}

fn gap_before(spec: &ButtonSpec) -> f32 {
    if spec.gap_before {
        GROUP_GAP
    } else {
        BUTTON_GAP
    }
}

/// 左のグループの幅。
fn left_width(named: bool) -> f32 {
    let gaps: f32 = LEFT.iter().skip(1).map(gap_before).sum();
    LEFT.iter().map(|s| button_w(s, named)).sum::<f32>() + gaps
}

/// 右のグループのボタンの幅の和と、ボタンの間（スライダーを挟まない 2 か所）の間隔。
fn zoom_buttons_width(named: bool) -> f32 {
    [&ZOOM_OUT, &ZOOM_IN, &FIT]
        .into_iter()
        .map(|s| button_w(s, named))
        .sum::<f32>()
        + 2.0 * BUTTON_GAP
}

/// ツールバー `bar` の部品の矩形。全部の名前とスライダーの下限が入るときだけ名前を出し、入らなければアイコンだけにする。
/// アイコンでも左のグループが右のグループに掛かるなら、掛かるボタンを出さない。スライダーは残りの幅が下限に足りなければ出さない。
pub(crate) fn cells(bar: Rect) -> Cells {
    let row = Rect::new(
        bar.x + PAD,
        bar.y + (bar.h - BUTTON_H).max(0.0) / 2.0,
        (bar.w - 2.0 * PAD).max(0.0),
        BUTTON_H.min(bar.h),
    );
    let slider_extra = |w: f32| w + BUTTON_GAP;
    let named =
        left_width(true) + GROUP_GAP + zoom_buttons_width(true) + slider_extra(SLIDER_MIN_W)
            <= row.w;
    let (left_w, zoom_w) = (left_width(named), zoom_buttons_width(named));
    let room = row.w - left_w - GROUP_GAP - zoom_w - BUTTON_GAP;
    let slider_w = if room >= SLIDER_MIN_W {
        room.min(SLIDER_W)
    } else {
        0.0
    };
    let zoom_total = zoom_w
        + if slider_w > 0.0 {
            slider_extra(slider_w)
        } else {
            0.0
        };
    let mut out = Cells {
        buttons: Vec::new(),
        slider: None,
        separators: Vec::new(),
    };
    left_buttons(&mut out, row, named, row.right() - zoom_total - GROUP_GAP);
    zoom_group(&mut out, row, named, (row.right() - zoom_total, slider_w));
    out
}

/// 左のグループを `limit` の左に収まる所まで並べる。
fn left_buttons(out: &mut Cells, row: Rect, named: bool, limit: f32) {
    let mut x = row.x;
    for (i, spec) in LEFT.iter().enumerate() {
        if i > 0 {
            x += gap_before(spec);
        }
        let rect = Rect::new(x, row.y, button_w(spec, named), row.h);
        x = rect.right();
        if rect.right() > limit {
            break;
        }
        if i > 0 && spec.gap_before {
            out.separators.push(separator(rect.x, row));
        }
        out.buttons.push(ButtonCell { spec, rect, named });
    }
}

/// 右のグループを `from` の x から並べる。`slider_w` が 0 ならスライダーを置かない。
fn zoom_group(out: &mut Cells, row: Rect, named: bool, (from, slider_w): (f32, f32)) {
    out.separators.push(separator(from, row));
    let mut x = from;
    push_button(out, row, named, &mut x, &ZOOM_OUT);
    if slider_w > 0.0 {
        let rect = Rect::new(x, row.y, slider_w, row.h);
        let track = Rect::new(rect.x + THUMB_W / 2.0, rect.y, slider_w - THUMB_W, rect.h);
        out.slider = Some(Slider { rect, track });
        x = rect.right() + BUTTON_GAP;
    }
    push_button(out, row, named, &mut x, &ZOOM_IN);
    push_button(out, row, named, &mut x, &FIT);
}

/// ボタンを `x` に置き、`x` を次のボタンの位置へ進める。
fn push_button(out: &mut Cells, row: Rect, named: bool, x: &mut f32, spec: &'static ButtonSpec) {
    let rect = Rect::new(*x, row.y, button_w(spec, named), row.h);
    *x = rect.right() + BUTTON_GAP;
    out.buttons.push(ButtonCell { spec, rect, named });
}

/// グループの間の縦線。`x` は右のグループの左端で、線は間隔の中央。
fn separator(x: f32, row: Rect) -> Rect {
    Rect::new(
        x - GROUP_GAP / 2.0,
        row.y + 4.0,
        1.0,
        (row.h - 8.0).max(0.0),
    )
}

/// スライダーの位置（0 が縮小の下限 `floor`、1 が [`MAX_PX_PER_SEC`]）。秒あたりのピクセルの対数に比例する。
pub fn zoom_fraction(px_per_sec: f64, floor: f64) -> f32 {
    let lo = floor.max(f64::MIN_POSITIVE);
    if MAX_PX_PER_SEC <= lo {
        return 0.0;
    }
    ((px_per_sec.max(lo) / lo).ln() / (MAX_PX_PER_SEC / lo).ln()).clamp(0.0, 1.0) as f32
}

/// スライダーの位置 `fraction` の、秒あたりのピクセル。[`zoom_fraction`] の逆。
pub fn zoom_px(fraction: f32, floor: f64) -> f64 {
    let lo = floor.max(f64::MIN_POSITIVE);
    if MAX_PX_PER_SEC <= lo {
        return lo;
    }
    lo * (MAX_PX_PER_SEC / lo).powf(f64::from(fraction.clamp(0.0, 1.0)))
}

/// ツールバー `bar` の `(x, y)` の下の、押せる部品。
pub(crate) fn part_in(bar: Rect, x: f32, y: f32) -> Option<ToolbarPart> {
    let c = cells(bar);
    if c.slider.is_some_and(|s| s.rect.contains(x, y)) {
        return Some(ToolbarPart::Slider);
    }
    c.buttons
        .iter()
        .find(|b| b.rect.contains(x, y))
        .and_then(|b| b.spec.command)
        .map(ToolbarPart::Button)
}

/// ボタン `command` の矩形（`bar` と同じ座標）。そのボタンが無いか、幅が足りずに出していなければ `None`。
pub fn toolbar_button(bar: Rect, command: &str) -> Option<Rect> {
    cells(bar)
        .buttons
        .into_iter()
        .find(|b| b.spec.command == Some(command))
        .map(|b| b.rect)
}

#[cfg(test)]
mod tests;
