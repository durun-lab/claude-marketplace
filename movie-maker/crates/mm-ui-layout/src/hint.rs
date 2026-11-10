//! ポインターの下にあるものと、そこから決まる手がかり（カーソルの形、ステータスバーの文、ツールチップ）。
//!
//! 描画のスレッドの `Ui` が [`Pointee`] を作り、UI のスレッド（`WM_SETCURSOR`、ツールチップ、右クリックのメニュー）が
//! 同じ型で引く。当たった所は [`Under`] 1 つに持たせ、レイヤーとカットの添字も含める。
//! 修飾キー（Alt、Shift）は UI のスレッドが読む瞬間の値を [`cursor_for`] に渡す（押した・離しただけでカーソルを替えるため）。

mod bar;
mod status;
mod tooltip;

pub use bar::status_labels;
pub use status::{status_left, status_right, HintText};
pub use tooltip::{bar_tooltip, tooltip_of, TooltipContent};

use crate::controller::Mods;
use crate::screen::Splitter;
use crate::timeline::{Hit, Part};
use crate::tool::Tool;
use crate::toolbar::ToolbarPart;
use crate::transport::{TransportPart, Viewer};
use crate::zoombar::ZoomBarPart;

/// ポインターの下の部品。Timeline の当たった所に、ツールバーと再生の操作のバーの部品を加えたもの。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Under {
    /// 何も無い（サーフェスの外、映像の領域）。
    #[default]
    Outside,
    Timeline(Hit),
    Toolbar(ToolbarPart),
    Transport(Viewer, TransportPart),
}

/// 編集のドラッグの種類。ドラッグ中のカーソルと、ステータスバーの操作の名前を決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditDrag {
    TrimLayer,
    Ripple,
    Roll,
    Slip,
    Slide,
    MoveInTime,
    /// キーフレームの行のキーフレームのドラッグ。
    MoveKeyframe,
}

impl EditDrag {
    /// ステータスバーに出す操作の名前。
    pub fn name(self) -> &'static str {
        match self {
            Self::TrimLayer => "Trim Layer",
            Self::Ripple => "Ripple Trim",
            Self::Roll => "Roll",
            Self::Slip => "Slip",
            Self::Slide => "Slide",
            Self::MoveInTime => "Move in Time",
            Self::MoveKeyframe => "Move Keyframe",
        }
    }
}

/// ドラッグ中の編集の様子。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dragging {
    pub kind: EditDrag,
    /// ポインターの向きに動かした量（秒。まだ動かしていなければ `None`）。
    pub by: Option<f64>,
    /// 吸い付いた先の文（`SnapTarget::message`）。
    pub snap: Option<&'static str>,
}

/// ポインターの下にあるもの。カーソル、ステータスバー、ツールチップ、右クリックのメニューが同じ値から決める。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pointee {
    pub under: Under,
    pub tool: Tool,
    /// 編集のドラッグ中。
    pub dragging: Option<Dragging>,
    /// ポインターの下のカットが隙間（割るものが無い）。
    pub gap: bool,
    /// Blade で、ポインターの下に割れる位置がある。
    pub blade_ready: bool,
}

/// カーソルの形。左右と上下の矢印と禁止は Windows の標準、roll、slip、slide、刃は自前の画像（`win/cursor.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
    SizeWE,
    /// 上下の矢印。Program と Timeline の間の横の仕切りの上だけで出す。
    SizeNS,
    No,
    Roll,
    Slip,
    Slide,
    Blade,
}

/// 仕切りの上のカーソル。サーフェスと WebView の間の縦の仕切りは左右、Program と Timeline の間の横の仕切りは上下の矢印。
pub fn splitter_cursor(which: Splitter) -> CursorKind {
    match which {
        Splitter::Vertical => CursorKind::SizeWE,
        Splitter::Horizontal => CursorKind::SizeNS,
    }
}

/// 帯の端のカーソル。カットの端は Alt で roll、ほかは左右の矢印。
fn edge_cursor(cut: bool, mods: Mods) -> CursorKind {
    if cut && mods.alt {
        CursorKind::Roll
    } else {
        CursorKind::SizeWE
    }
}

/// ポインターの下 `p` と今押している修飾キー `mods` から決めるカーソル。
pub fn cursor_for(p: &Pointee, mods: Mods) -> CursorKind {
    if let Some(d) = p.dragging {
        return match d.kind {
            EditDrag::TrimLayer | EditDrag::Ripple => CursorKind::SizeWE,
            EditDrag::Roll => CursorKind::Roll,
            EditDrag::Slip => CursorKind::Slip,
            EditDrag::Slide => CursorKind::Slide,
            EditDrag::MoveInTime | EditDrag::MoveKeyframe => CursorKind::Arrow,
        };
    }
    let Under::Timeline(hit) = p.under else {
        return CursorKind::Arrow;
    };
    match (hit, p.tool) {
        (Hit::Layer { .. }, Tool::Blade) => CursorKind::No,
        (Hit::Cut { .. }, Tool::Blade) if p.gap || !p.blade_ready => CursorKind::No,
        (Hit::Cut { .. }, Tool::Blade) => CursorKind::Blade,
        (Hit::Layer { part, .. }, _) if part != Part::Body => edge_cursor(false, mods),
        (Hit::Cut { part, .. }, _) if part != Part::Body => edge_cursor(true, mods),
        // 隙間は slip できないが、Shift で slide できる（`gesture::body_edit`）。
        (Hit::Cut { .. }, Tool::Trim) if mods.shift => CursorKind::Slide,
        (Hit::Cut { .. }, Tool::Trim) if !p.gap => CursorKind::Slip,
        (
            Hit::ZoomBar {
                part: ZoomBarPart::StartEdge | ZoomBarPart::EndEdge,
            },
            _,
        ) => CursorKind::SizeWE,
        _ => CursorKind::Arrow,
    }
}

#[cfg(test)]
mod tests;
