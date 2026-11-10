//! Timeline の上端のツールバー。左から Undo と Redo、ツール（Selection、Trim、Blade）、Split Clip、Lift、Extract、
//! Snapping、右に Zoom Out、スライダー、Zoom In、Full Extent Zoom を並べる（Resolve と CapCut の Timeline の上端と同じ）。
//! 座標は渡された矩形（Timeline の割り付けの `toolbar`）と同じ。
//!
//! ボタンの矩形は [`cells`] 1 か所で決め、描く関数（`paint/toolbar.rs`、[`toolbar_labels`]）、当たり判定
//! （[`ToolbarController`]）、`--play-test` のクリックの位置（[`toolbar_button`]）が同じ値を見る。
//! 名前の幅は文字の仕組みで測った値（[`ToolbarNames`]）を使い、入らなければグループごとに名前を省く。
//! ボタンはコマンドの表（アプリの `commands.rs`）の ID を持ち、押して同じボタンの上で離すと、その ID を
//! [`ToolbarAction::Command`] で返す。押せるかはアプリがコマンドの表から作った [`ToolbarState::disabled`]、Snapping の
//! 入の見た目は [`ToolbarState::snapping`] で決める。
//! スライダーは秒あたりの論理ピクセルを対数で動かす（[`zoom_fraction`]、[`zoom_px`]）。

mod cells;
mod controller;
mod icons;
mod labels;
mod look;

use crate::geom::Rect;
use crate::text::{TextMeasure, UiFont};
use crate::time_view::MAX_PX_PER_SEC;

pub(crate) use cells::{cells, ButtonCell, Slider, THUMB_W};
pub use controller::{ToolbarAction, ToolbarController, ToolbarPart};
pub(crate) use icons::{icon_quads, Icon};
pub use labels::toolbar_labels;
pub(crate) use look::ButtonLook;

/// スナップのコマンドの ID。このボタンだけが入/切の見た目を持つ。
const SNAPPING: &str = "timeline.toggle_snapping";

/// ボタンのグループ。左から並べ、グループの間に区切りの縦線を引く。ズームのグループだけは右端に寄せる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    History,
    Tools,
    Edit,
    Snap,
    Zoom,
}

/// 幅が足りないときに名前を省くグループの順（先頭から省く）。取り消しの矢印はどのアプリでも同じ形で通じるので先に省き、
/// 編集（Lift と Extract）はアイコンだけでは見分けにくいので最後まで残す。
pub(crate) const NAME_DROP_ORDER: [Group; 5] = [
    Group::History,
    Group::Zoom,
    Group::Tools,
    Group::Snap,
    Group::Edit,
];

/// ボタンの定義。`command` はコマンドの表の ID。ツールのボタンは Z4 までコマンドを持たず、場所だけを空けておく。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ButtonSpec {
    pub command: Option<&'static str>,
    pub icon: Icon,
    /// 幅が足りるときにアイコンの横に出す短い名前。空ならアイコンだけ。
    pub name: &'static str,
    pub group: Group,
    /// 常に入の見た目にする（今のツール）。
    pub lit: bool,
}

const fn button(command: &'static str, icon: Icon, name: &'static str, group: Group) -> ButtonSpec {
    ButtonSpec {
        command: Some(command),
        icon,
        name,
        group,
        lit: false,
    }
}

/// ツールのボタン。Selection だけが今のツールで、Trim と Blade は Z4 まで押せない。
const fn tool(icon: Icon, name: &'static str, lit: bool) -> ButtonSpec {
    ButtonSpec {
        command: None,
        icon,
        name,
        group: Group::Tools,
        lit,
    }
}

/// 全部のボタンを左から並べる順に。ズームのグループは Zoom Out と Zoom In の間にスライダーを挟み、Zoom Out と Zoom In は
/// 名前を持たない（スライダーの両端の虫眼鏡で通じる）。Full Extent Zoom の名前は「Fit」。
pub(crate) static BUTTONS: [ButtonSpec; 12] = [
    button(
        "edit.undo",
        Icon::glyph('\u{E7A7}', '\u{21B6}'),
        "Undo",
        Group::History,
    ),
    button(
        "edit.redo",
        Icon::glyph('\u{E7A6}', '\u{21B7}'),
        "Redo",
        Group::History,
    ),
    tool(Icon::Selection, "Selection", true),
    tool(Icon::Trim, "Trim", false),
    tool(Icon::Blade, "Blade", false),
    button(
        "clip.split",
        Icon::glyph('\u{E8C6}', '\u{2704}'),
        "Split",
        Group::Edit,
    ),
    button("clip.lift", Icon::Lift, "Lift", Group::Edit),
    button("clip.extract", Icon::Extract, "Extract", Group::Edit),
    button(SNAPPING, Icon::Snapping, "Snapping", Group::Snap),
    button(
        "timeline.zoom_out",
        Icon::glyph('\u{E71F}', '\u{2212}'),
        "",
        Group::Zoom,
    ),
    button(
        "timeline.zoom_in",
        Icon::glyph('\u{E8A3}', '+'),
        "",
        Group::Zoom,
    ),
    button(
        "timeline.full_extent_zoom",
        Icon::glyph('\u{E740}', '\u{2922}'),
        "Fit",
        Group::Zoom,
    ),
];

/// ボタンの名前の幅（論理ピクセル、[`BUTTONS`] の順）。字と字の大きさで変わるので見積もらず、`Ui` が文字の仕組みで測る。
/// 測る前は空で、どのボタンも名前を出さない。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolbarNames(Vec<f32>);

impl ToolbarNames {
    /// 名前を字 `font` の地の文の大きさで測る。
    pub fn measure(font: &UiFont, measure: &mut dyn TextMeasure) -> Self {
        Self(
            BUTTONS
                .iter()
                .map(|b| measure.width(b.name, font.base))
                .collect(),
        )
    }

    /// `i` 番目のボタンの名前の幅。名前が無いか、まだ測っていなければ `None`。
    pub(crate) fn width(&self, i: usize) -> Option<f32> {
        self.0.get(i).copied().filter(|w| *w > 0.0)
    }
}

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
    /// スナップが入（Snapping を入の見た目にする）。
    pub snapping: bool,
    /// 今の秒あたりの論理ピクセルと、縮小の下限（`TimeView::zoom_floor`）。スライダーの位置と、ドラッグの換算に使う。
    pub px_per_sec: f64,
    pub floor: f64,
    /// ボタンの名前の幅。
    pub names: ToolbarNames,
    /// Windows の記号のフォント（Segoe Fluent Icons か Segoe MDL2 Assets）がある。無ければ字のアイコンを標準の記号の字で描く。
    pub symbols: bool,
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
pub(crate) fn part_in(bar: Rect, names: &ToolbarNames, x: f32, y: f32) -> Option<ToolbarPart> {
    let c = cells(bar, names);
    if c.slider.is_some_and(|s| s.rect.contains(x, y)) {
        return Some(ToolbarPart::Slider);
    }
    c.buttons
        .iter()
        .find(|b| b.rect.contains(x, y))
        .and_then(|b| b.spec.command)
        .map(ToolbarPart::Button)
}

/// ボタン `command` の矩形（`bar` と同じ座標）。`names` は描いたときの名前の幅。そのボタンが無いか、幅が足りずに
/// 出していなければ `None`。
pub fn toolbar_button(bar: Rect, names: &ToolbarNames, command: &str) -> Option<Rect> {
    cells(bar, names)
        .buttons
        .into_iter()
        .find(|b| b.spec.command == Some(command))
        .map(|b| b.rect)
}

#[cfg(test)]
mod tests;
