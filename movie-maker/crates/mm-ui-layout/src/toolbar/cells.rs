//! ツールバーの部品の矩形。名前は入る所まで出し、入らなければ [`NAME_DROP_ORDER`] の順にグループごとに省く（Windows の
//! リボンの ScalingPolicy が、幅に合わせて決めた順にグループを縮めるのと同じ）。アイコンだけでも入らなければスライダーを出さずにボタンを縮め
//! （再生の操作のバーと同じ）、それでも入らなければ右のグループに掛かる左のボタンを出さない。

use super::{ButtonSpec, Group, ToolbarNames, BUTTONS, NAME_DROP_ORDER};
use crate::geom::Rect;

/// ボタンの高さ。ツールバーの高さの中で縦に中央へ置く。
const BUTTON_H: f32 = 24.0;
/// ツールバーの左右の余白。
const PAD: f32 = 8.0;
/// ボタンの間隔とグループの間隔。
const BUTTON_GAP: f32 = 2.0;
const GROUP_GAP: f32 = 12.0;
/// アイコンだけのボタンの幅と、幅が足りないときに縮める下限（アイコンの欄の左右に 3 ずつ）。
const ICON_BUTTON_W: f32 = 28.0;
const MIN_ICON_BUTTON_W: f32 = 22.0;
/// 名前のあるボタンの、アイコンの欄の幅、アイコンと名前の間隔、左右の余白。
const ICON_W: f32 = 16.0;
const NAME_GAP: f32 = 4.0;
const NAME_PAD: f32 = 6.0;
/// スライダーの幅（欲しい幅と、これより狭ければ出さない下限）と、スライダーのつまみの幅。
const SLIDER_W: f32 = 110.0;
const SLIDER_MIN_W: f32 = 56.0;
pub(crate) const THUMB_W: f32 = 10.0;
/// 幅を比べるときの誤差。ちょうど入る幅が、足し引きの丸めで入らないことにならないようにする。
const EPS: f32 = 0.01;

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

    /// 名前の欄。右の余白まで含め、測った幅がそのまま入る（足し引きの丸めで「…」にならない）。名前を出さないときは `None`。
    pub(crate) fn name_box(&self) -> Option<Rect> {
        let x = self.icon_box().right() + NAME_GAP;
        let w = self.rect.right() - x;
        (self.named && w > 0.0).then(|| Rect::new(x, self.rect.y, w, self.rect.h))
    }
}

/// スライダー。`rect` がクリックを受ける矩形、`track` がスライダーの溝（つまみの中心が動く範囲）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slider {
    pub rect: Rect,
    pub track: Rect,
}

impl Slider {
    /// スライダーの溝の `x` の位置（左端が 0、右端が 1）。
    pub(crate) fn fraction_at(&self, x: f32) -> f32 {
        if self.track.w <= 0.0 {
            return 0.0;
        }
        ((x - self.track.x) / self.track.w).clamp(0.0, 1.0)
    }

    /// 位置 `fraction` のスライダーのつまみの中心の x。
    pub(crate) fn thumb_x(&self, fraction: f32) -> f32 {
        self.track.x + self.track.w * fraction
    }
}

/// ツールバーの部品の矩形。
#[derive(Debug, Clone, Default)]
pub(crate) struct Cells {
    pub buttons: Vec<ButtonCell>,
    /// 幅が足りないときは `None`。
    pub slider: Option<Slider>,
    /// グループの間の縦線。
    pub separators: Vec<Rect>,
}

/// 並べ方。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Plan {
    /// 名前を省くグループの数（[`NAME_DROP_ORDER`] の先頭から）。
    dropped: usize,
    /// アイコンだけのボタンの幅。
    icon_w: f32,
    /// スライダーの幅。0 なら出さない。
    slider_w: f32,
}

impl Plan {
    /// `i` 番目のボタンの幅と、名前を出すか。
    fn button(&self, i: usize, names: &ToolbarNames) -> (f32, bool) {
        let named = !NAME_DROP_ORDER[..self.dropped].contains(&BUTTONS[i].group);
        match names.width(i).filter(|_| named) {
            Some(w) => (NAME_PAD * 2.0 + ICON_W + NAME_GAP + w, true),
            None => (self.icon_w, false),
        }
    }
}

/// 隣り合うボタンの間隔。グループの切れ目は広くする（左のグループとズームのグループの間は、これが最小）。
fn gap(prev: &ButtonSpec, next: &ButtonSpec) -> f32 {
    if prev.group == next.group {
        BUTTON_GAP
    } else {
        GROUP_GAP
    }
}

/// ボタンの間隔の和（スライダーの脇の分を除く）。
fn gaps() -> f32 {
    BUTTONS.windows(2).map(|w| gap(&w[0], &w[1])).sum()
}

/// ズームのグループの最初のボタンの添字。
fn zoom_start() -> usize {
    BUTTONS
        .iter()
        .position(|b| b.group == Group::Zoom)
        .unwrap_or(BUTTONS.len())
}

/// 行の幅 `w` での並べ方。名前を省くグループの少ない方から試し、全部のボタンとスライダーの下限が入るものを採る。
fn plan(w: f32, names: &ToolbarNames) -> Plan {
    for dropped in 0..=NAME_DROP_ORDER.len() {
        let p = Plan {
            dropped,
            icon_w: ICON_BUTTON_W,
            slider_w: 0.0,
        };
        let need = (0..BUTTONS.len())
            .map(|i| p.button(i, names).0)
            .sum::<f32>()
            + gaps();
        if need + BUTTON_GAP + SLIDER_MIN_W <= w + EPS {
            let slider_w = (w - need - BUTTON_GAP).min(SLIDER_W);
            return Plan { slider_w, ..p };
        }
    }
    let icon_w = ((w - gaps()) / BUTTONS.len() as f32).clamp(MIN_ICON_BUTTON_W, ICON_BUTTON_W);
    Plan {
        dropped: NAME_DROP_ORDER.len(),
        icon_w,
        slider_w: 0.0,
    }
}

/// ツールバー `bar` の部品の矩形。`names` は測った名前の幅。
pub(crate) fn cells(bar: Rect, names: &ToolbarNames) -> Cells {
    let row = Rect::new(
        bar.x + PAD,
        bar.y + (bar.h - BUTTON_H).max(0.0) / 2.0,
        (bar.w - 2.0 * PAD).max(0.0),
        BUTTON_H.min(bar.h),
    );
    let plan = plan(row.w, names);
    let zoom = zoom_start();
    let from = row.right() - zoom_width(&plan, names, zoom);
    let mut out = Cells::default();
    left_buttons(&mut out, (row, from - GROUP_GAP), &plan, names, zoom);
    zoom_group(&mut out, (row, from), &plan, names, zoom);
    out
}

/// ズームのグループ（`zoom` 番目から最後まで）の幅。スライダーは最初のボタンの後ろに挟む。
fn zoom_width(plan: &Plan, names: &ToolbarNames, zoom: usize) -> f32 {
    let buttons: f32 = (zoom..BUTTONS.len()).map(|i| plan.button(i, names).0).sum();
    let gaps = (BUTTONS.len() - zoom).saturating_sub(1) as f32 * BUTTON_GAP;
    let slider = if plan.slider_w > 0.0 {
        plan.slider_w + BUTTON_GAP
    } else {
        0.0
    };
    buttons + gaps + slider
}

/// ズームより左のボタンを、`limit` の左に収まる所まで行の左端から並べる。
fn left_buttons(
    out: &mut Cells,
    (row, limit): (Rect, f32),
    plan: &Plan,
    names: &ToolbarNames,
    zoom: usize,
) {
    let mut x = row.x;
    for (i, spec) in BUTTONS[..zoom].iter().enumerate() {
        let new_group = i > 0 && BUTTONS[i - 1].group != spec.group;
        if i > 0 {
            x += gap(&BUTTONS[i - 1], spec);
        }
        let (w, named) = plan.button(i, names);
        let rect = Rect::new(x, row.y, w, row.h);
        if rect.right() > limit + EPS {
            break;
        }
        if new_group {
            out.separators.push(separator(rect.x, row));
        }
        out.buttons.push(ButtonCell { spec, rect, named });
        x = rect.right();
    }
}

/// ズームのグループを `from` の x から並べる。最初のボタン（Zoom Out）の後ろにスライダーを挟む。
fn zoom_group(
    out: &mut Cells,
    (row, from): (Rect, f32),
    plan: &Plan,
    names: &ToolbarNames,
    zoom: usize,
) {
    out.separators.push(separator(from, row));
    let mut x = from;
    for (i, spec) in BUTTONS.iter().enumerate().skip(zoom) {
        if i == zoom + 1 && plan.slider_w > 0.0 {
            let rect = Rect::new(x, row.y, plan.slider_w, row.h);
            let track = Rect::new(rect.x + THUMB_W / 2.0, rect.y, rect.w - THUMB_W, rect.h);
            out.slider = Some(Slider { rect, track });
            x = rect.right() + BUTTON_GAP;
        }
        let (w, named) = plan.button(i, names);
        let rect = Rect::new(x, row.y, w, row.h);
        out.buttons.push(ButtonCell { spec, rect, named });
        x = rect.right() + BUTTON_GAP;
    }
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

#[cfg(test)]
mod tests;
