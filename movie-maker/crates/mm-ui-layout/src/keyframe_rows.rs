//! キーフレームの行。レイヤーの行の下に、キーフレームのあるパラメータごとの行を開く（AE のプロパティの行、
//! Resolve の Keyframe Editor）。行の見出しはパラメータの名前、行の中はキーフレームのひし形。
//! 置き場所と当たり判定はここが決め、描く関数（`paint/keyframe_rows.rs`）、文字、操作（`controller/keyframe_rows.rs`）、
//! 自動試験のクリックの位置が同じ値を見る。座標は Timeline の矩形の左上を (0, 0) とする。

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::geom::Rect;
use crate::header;
use crate::paint::PaintState;
use crate::rows::KEYFRAME_ROW_H;
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::timeline::{Hit, Item, TimelineModel};
use crate::timeline_layout::Viewport;

/// ひし形の一辺（外接する正方形）。帯のマーカー（9）より大きくし、ドラッグで掴みやすくする。
pub const ROW_DIAMOND_PX: f32 = 11.0;
/// ひし形の当たり判定を、外接する正方形から広げる幅。
const HIT_SLACK: f32 = 2.0;
/// 行の見出しの名前の右の余白。
const NAME_PAD: f32 = 4.0;

/// キーフレームの行 1 本（キーフレームのあるパラメータ 1 つ）。
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeRow {
    /// 行の見出しに出す名前（「Transform › Position」）。
    pub name: Arc<str>,
    /// キーフレーム（前から順）。
    pub keyframes: Vec<RowKeyframe>,
}

/// キーフレームの行のキーフレーム 1 つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowKeyframe {
    /// 時刻（コンポジションの秒）。
    pub time: f64,
    /// ドラッグで動かせる範囲（コンポジションの秒、両端を含む）。再フレーミングのキーフレームはそのカットの中に留める。
    pub min: f64,
    pub max: f64,
}

/// キーフレームの行のキーフレーム 1 つの位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyframeIndex {
    /// レイヤーの添字（`TimelineModel::items`）。
    pub item: usize,
    /// キーフレームの行の添字（`Item::keyframe_rows`）。
    pub row: usize,
    /// 行の中のキーフレームの添字（`KeyframeRow::keyframes`）。
    pub keyframe: usize,
}

/// キーフレームの行で選んだもの。行の見出しを押したときは、キーフレームを持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowPick {
    pub item: usize,
    pub row: usize,
    pub keyframe: Option<usize>,
}

impl RowPick {
    /// 選んだキーフレーム。行だけを選んでいれば `None`。
    pub fn keyframe_index(self) -> Option<KeyframeIndex> {
        self.keyframe.map(|keyframe| KeyframeIndex {
            item: self.item,
            row: self.row,
            keyframe,
        })
    }
}

/// キーフレームの行の操作。受け取る側が行の添字からパラメータとキーフレームを引いて操作にする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyframeAction {
    /// キーフレームの行を開く・閉じる（表示の状態）。`Some(i)` はレイヤー i、`None` はキーフレームのある全部のレイヤー。
    ToggleRows(Option<usize>),
    /// キーフレームの行（とキーフレーム）を選んだ。選んだパラメータを Inspector と Graph でも選ぶ。外れたら `None`。
    Select(Option<RowPick>),
    /// キーフレームを時刻 `to`（コンポジションの秒、フレームの先頭）へ動かす（`moveKeyframe`）。
    Move { keyframe: KeyframeIndex, to: f64 },
    /// キーフレームを外す（`removeKeyframe`）。
    Remove(KeyframeIndex),
    /// 行の時刻 `time`（コンポジションの秒、フレームの先頭）に、その時刻のパラメータの値でキーフレームを打つ（`setKeyframe`）。
    Add { item: usize, row: usize, time: f64 },
}

/// 描く時点のキーフレームの行の状態。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KeyframeRowState {
    /// 選んだ行とキーフレーム。
    pub selected: Option<RowPick>,
    /// ドラッグ中のキーフレームと、動かした先の時刻（コンポジションの秒）。
    pub dragging: Option<(KeyframeIndex, f64)>,
}

/// 行 `i` の下のキーフレームの行 `k` の、行の領域の幅の矩形。上端の 1 px は行どうしの境の線。
pub(crate) fn row_rect(vp: &Viewport, i: usize, k: usize) -> Rect {
    let area = vp.layout.row_area;
    let y = area.y + vp.rows.keyframe_row_top(i, k) - vp.scroll_y;
    Rect::new(area.x, y, area.w, KEYFRAME_ROW_H)
}

/// 行 `i` の下のキーフレームの行 `k` の、見出しの列の幅の矩形。
pub(crate) fn header_rect(vp: &Viewport, i: usize, k: usize) -> Rect {
    let row = row_rect(vp, i, k);
    let headers = vp.layout.headers;
    Rect::new(headers.x, row.y, headers.w, row.h)
}

/// 行 `i` の下のキーフレームの行 `k` の、時刻 `t` のひし形の中心（Timeline の座標）。自動試験もこの位置を押す。
pub fn keyframe_center(vp: &Viewport, (i, k): (usize, usize), t: f64) -> (f32, f32) {
    let row = row_rect(vp, i, k);
    (vp.x_of(t), row.y + (row.h + 1.0) / 2.0)
}

/// 中心 `center`、一辺 `size` の正方形。
pub(crate) fn square((x, y): (f32, f32), size: f32) -> Rect {
    Rect::new(x - size / 2.0, y - size / 2.0, size, size)
}

/// 縦の位置 `y`（Timeline の座標）が行 `i` の下のキーフレームの行に入っていれば、その行の添字。
pub(crate) fn row_at(vp: &Viewport, i: usize, y: f32) -> Option<usize> {
    let content = y - vp.layout.row_area.y + vp.scroll_y;
    vp.rows.keyframe_row_at(i, content)
}

/// 行 `index` のレイヤーの、行の領域の点 (x, y) に当たるもの。ひし形が重なる所では、中心が近い方。
/// 点がキーフレームの行に無ければ `None`。
pub(crate) fn hit(vp: &Viewport, (index, item): (usize, &Item), (x, y): (f32, f32)) -> Option<Hit> {
    let row = row_at(vp, index, y)?;
    let keys = &item.keyframe_rows.get(row)?.keyframes;
    let near = keys
        .iter()
        .map(|k| keyframe_center(vp, (index, row), k.time))
        .enumerate()
        .filter(|(_, c)| square(*c, ROW_DIAMOND_PX + 2.0 * HIT_SLACK).contains(x, y))
        .min_by(|a, b| (a.1 .0 - x).abs().total_cmp(&(b.1 .0 - x).abs()));
    Some(
        near.map_or(Hit::KeyframeRow { item: index, row }, |(keyframe, _)| {
            Hit::RowKeyframe(KeyframeIndex {
                item: index,
                row,
                keyframe,
            })
        }),
    )
}

/// キーフレームの行を開閉する。`target` が `Some(i)` ならレイヤー i、`None` ならキーフレームの行のある全部のレイヤー
/// （どれかが閉じていれば全部を開き、全部開いていれば全部を閉じる。AE の U でレイヤーを選んでいないときと同じ）。開いたら真。
pub fn toggle_open(open: &mut BTreeSet<String>, items: &[Item], target: Option<usize>) -> bool {
    let Some(i) = target else {
        let keyed: Vec<&Item> = items
            .iter()
            .filter(|it| !it.keyframe_rows.is_empty())
            .collect();
        let closed = keyed.iter().any(|it| !open.contains(&*it.id));
        for it in keyed {
            if closed {
                open.insert(it.id.to_string());
            } else {
                open.remove(&*it.id);
            }
        }
        return closed;
    };
    let Some(item) = items.get(i) else {
        return false;
    };
    let opened = !open.remove(&*item.id);
    if opened {
        open.insert(item.id.to_string());
    }
    opened
}

/// 見えているキーフレームの行の見出しの名前。レイヤーの見出しの名前と左をそろえ、選んだ行は選択の色にする。
pub(crate) fn labels(
    model: &TimelineModel,
    vp: &Viewport,
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let area = vp.layout.headers;
    let picked = state.keyframe_rows.selected;
    let mut out = Vec::new();
    for i in vp.visible_rows() {
        let Some(item) = model.items.get(i) else {
            break;
        };
        let name_x = header::cells(header::row_rect(vp, i), item).name.x;
        let open = vp.rows.keyframe_rows(i);
        for (k, row) in item.keyframe_rows.iter().enumerate().take(open) {
            let r = header_rect(vp, i, k);
            let cell = Rect::new(name_x, r.y + 1.0, r.right() - NAME_PAD - name_x, r.h - 1.0);
            let selected = picked.is_some_and(|p| (p.item, p.row) == (i, k));
            let p = &state.palette;
            let color = if selected { p.selection } else { p.label };
            let style = TextStyle {
                size: font.small,
                color,
            };
            let Some(clip) = header::inside(cell, area) else {
                continue;
            };
            if let Some(mut label) = place(&row.name, cell, 0.0, Align::Left, style, measure) {
                label.clip = clip;
                out.push(label);
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests;
