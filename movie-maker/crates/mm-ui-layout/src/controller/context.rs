//! 右クリックのメニューの前処理。ポインターの下のカットとレイヤーを先に選び（Premiere と Resolve と同じ。メニューの項目が
//! 選んだものに効く）、当たった所と、右クリックした時刻、メニューを出す点を [`Action::ContextMenu`] で返す。
//! キーボードのメニューキー（Shift+F10、アプリケーションキー）は、選んでいるものの位置を点にする。

use super::{deselect_cut, Action, Drag, TimelineController};
use crate::pointed::Place;
use crate::snap::quantize;
use crate::timeline::{hit_test, Hit, TimelineModel};

/// メニューキーでメニューを出す点の、目盛りの帯の下端からの距離（論理ピクセル）。何も選んでいないときの点。
const BELOW_RULER_PX: f32 = 4.0;

impl TimelineController {
    /// 点 `(x, y)`（Timeline の座標）の右クリック。ドラッグ中、メニューを出さない所（角、スクロールバー、ズームバー）では何も返さない。
    pub(super) fn context_menu_at(
        &mut self,
        (x, y): (f32, f32),
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        if self.drag != Drag::None {
            return Vec::new();
        }
        let hit = hit_test(model, &self.viewport(), x, y);
        let Some(place) = Place::of(&hit) else {
            return Vec::new();
        };
        let time = self.blade_time(x, model, playhead).0;
        let mut out = self.select_under(hit);
        out.push(Action::ContextMenu {
            place,
            time: time.clamp(0.0, model.duration),
            at: (x, y),
        });
        out
    }

    /// メニューキー。選んでいるカット、レイヤーの順にその位置でメニューを出し、何も選んでいなければ再生位置の下の空きに出す。
    pub(super) fn context_menu_key(&mut self, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        if self.drag != Drag::None {
            return Vec::new();
        }
        let vp = self.viewport();
        let cut = self
            .selected_cut
            .and_then(|(item, cut)| Some((item, model.items.get(item)?.cuts.get(cut)?)));
        let (place, time, at) = if let Some((item, bar)) = cut {
            let time = if (bar.start..bar.end).contains(&playhead) {
                playhead
            } else {
                (bar.start + bar.end) / 2.0
            };
            (Place::Cut, time, (vp.x_of(time), row_center(&vp, item)))
        } else if let Some((item, it)) = self.selected.and_then(|i| Some((i, model.items.get(i)?)))
        {
            let time = playhead.clamp(it.start, it.end);
            (Place::Layer, time, (vp.x_of(time), row_center(&vp, item)))
        } else {
            let y = self.layout.ruler.bottom() + BELOW_RULER_PX;
            (Place::Empty, playhead, (vp.x_of(playhead), y))
        };
        let time = quantize(time, model.fps).clamp(0.0, model.duration);
        vec![Action::ContextMenu { place, time, at }]
    }

    /// ポインターの下のカットかレイヤーを選ぶ。目盛りと空きでは選択を変えない（今の選択に効くメニューを出す）。
    fn select_under(&mut self, hit: Hit) -> Vec<Action> {
        let had_cut = self.selected_cut;
        match hit {
            Hit::Cut { item, cut, .. } => {
                self.selected = Some(item);
                self.selected_cut = Some((item, cut));
                vec![
                    Action::Select(Some(item)),
                    Action::SelectCut(self.selected_cut),
                ]
            }
            Hit::Layer { index, .. } | Hit::Header { index } | Hit::HeaderButton { index, .. } => {
                self.select_layer_only(index, had_cut)
            }
            Hit::BarMark { item, .. } => self.select_layer_only(item, had_cut),
            _ => Vec::new(),
        }
    }

    fn select_layer_only(&mut self, index: usize, had_cut: Option<(usize, usize)>) -> Vec<Action> {
        self.selected = Some(index);
        self.selected_cut = None;
        deselect_cut(Action::Select(Some(index)), had_cut)
    }
}

/// 行 `item` の縦の中央。
fn row_center(vp: &crate::timeline_layout::Viewport<'_>, item: usize) -> f32 {
    let row = vp.row(item);
    row.y + row.h / 2.0
}
