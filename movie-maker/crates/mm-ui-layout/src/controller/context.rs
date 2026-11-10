//! 右クリックのメニューの前処理。ポインターの下のカット、レイヤー、キーフレームの行を先に選び（Premiere と Resolve と同じ。
//! メニューの項目が選んだものに効く）、当たった所と、Split Clip Here の割る時刻、メニューを出す点を [`Action::ContextMenu`]
//! で返す。キーボードのメニューキー（Shift+F10、アプリケーションキー）は、選んでいるものの位置を点にする。

use super::tools::splits;
use super::{deselect_cut, Action, Drag, TimelineController};
use crate::keyframe_rows::keyframe_center;
use crate::pointed::Place;
use crate::snap::quantize;
use crate::timeline::{hit_test, Hit, Part, TimelineModel};
use crate::timeline_layout::Viewport;

/// メニューキーでメニューを出す点の、目盛りの帯の下端からの距離（論理ピクセル）。何も選んでいないときの点。
const BELOW_RULER_PX: f32 = 4.0;

/// メニューキーで出すメニューの、当たった所、Split Clip Here の割る時刻、点。
type MenuKeyTarget = (Hit, Option<f64>, (f32, f32));

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
        if Place::of(&hit).is_none() {
            return Vec::new();
        }
        // Blade と同じ時刻で、割れる所でだけ持つ。吸い付いた再生位置が隣のカットにあれば、右クリックしたカットは割らない。
        let time = match hit {
            Hit::Cut { item, cut, .. } => self
                .blade_at((item, cut), x, model, playhead)
                .map(|line| line.time),
            _ => None,
        };
        let mut out = self.select_under(hit, model);
        out.push(Action::ContextMenu {
            hit,
            time,
            at: (x, y),
        });
        out
    }

    /// メニューキー。選んでいるキーフレームの行、カット、レイヤーの順にその位置でメニューを出し、何も選んでいなければ
    /// 再生位置の下の空きに出す。点は行の領域に収め、スクロールで隠れたものを選んでいても Timeline の中に出す。
    pub(super) fn context_menu_key(&mut self, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        if self.drag != Drag::None {
            return Vec::new();
        }
        let vp = self.viewport();
        let (hit, time, (x, y)) = self
            .menu_key_at_keyframe(&vp, model, playhead)
            .or_else(|| self.menu_key_at_cut(&vp, model, playhead))
            .or_else(|| self.menu_key_at_layer(&vp, model, playhead))
            .unwrap_or_else(|| {
                let y = self.layout.ruler.bottom() + BELOW_RULER_PX;
                (Hit::Empty, None, (vp.x_of(playhead), y))
            });
        let area = self.layout.row_area;
        let at = (
            x.clamp(area.x, area.right()),
            y.clamp(area.y, area.bottom()),
        );
        vec![Action::ContextMenu { hit, time, at }]
    }

    /// メニューキーで、選んでいるキーフレームの行（ひし形を選んでいればそのひし形）に出す所。
    fn menu_key_at_keyframe(
        &self,
        vp: &Viewport<'_>,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<MenuKeyTarget> {
        let (hit, t) = self.picked_for_menu(model, playhead)?;
        let (item, row) = match hit {
            Hit::RowKeyframe(at) => (at.item, at.row),
            Hit::KeyframeRow { item, row } => (item, row),
            _ => return None,
        };
        Some((hit, None, keyframe_center(vp, (item, row), t)))
    }

    /// メニューキーで、選んでいるカットに出す所。割る時刻は、再生位置で割れればそれ、割れなければカットの中央。
    fn menu_key_at_cut(
        &self,
        vp: &Viewport<'_>,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<MenuKeyTarget> {
        let (item, index) = self.selected_cut?;
        let bar = model.items.get(item)?.cuts.get(index)?;
        let time = [playhead, (bar.start + bar.end) / 2.0]
            .into_iter()
            .map(|t| quantize(t, model.fps))
            .find(|t| splits(bar, *t));
        let x = vp.x_of(time.unwrap_or_else(|| playhead.clamp(bar.start, bar.end)));
        let hit = Hit::Cut {
            item,
            cut: index,
            part: Part::Body,
        };
        Some((hit, time, (x, row_center(vp, item))))
    }

    /// メニューキーで、選んでいるレイヤーに出す所。再生位置をレイヤーの範囲に収めた時刻の位置。
    fn menu_key_at_layer(
        &self,
        vp: &Viewport<'_>,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<MenuKeyTarget> {
        let item = self.selected?;
        let it = model.items.get(item)?;
        let hit = Hit::Layer {
            index: item,
            part: Part::Body,
        };
        let x = vp.x_of(playhead.clamp(it.start, it.end));
        Some((hit, None, (x, row_center(vp, item))))
    }

    /// ポインターの下のものを選ぶ。カット、レイヤーの帯と見出しはそれを選び、左クリックと同じくキーフレームの行の選択を
    /// 外す。キーフレームの行はその行（とひし形）を選ぶ。目盛りと空きでは選択を変えない（今の選択に効くメニューを
    /// 出す）。
    fn select_under(&mut self, hit: Hit, model: &TimelineModel) -> Vec<Action> {
        if let Some(out) = self.pick_for_menu(hit, model) {
            return out;
        }
        let had_cut = self.selected_cut;
        let mut out = match hit {
            Hit::Cut { item, cut, .. } => {
                self.selected = Some(item);
                self.selected_cut = Some((item, cut));
                vec![
                    Action::Select(Some(item)),
                    Action::SelectCut(self.selected_cut),
                ]
            }
            Hit::Layer { index, .. }
            | Hit::Header { index }
            | Hit::HeaderButton { index, .. }
            | Hit::Disclosure { index } => self.select_layer_only(index, had_cut),
            Hit::BarMark { item, .. } => self.select_layer_only(item, had_cut),
            _ => return Vec::new(),
        };
        out.extend(self.drop_pick());
        out
    }

    fn select_layer_only(&mut self, index: usize, had_cut: Option<(usize, usize)>) -> Vec<Action> {
        self.selected = Some(index);
        self.selected_cut = None;
        deselect_cut(Action::Select(Some(index)), had_cut)
    }
}

/// 行 `item` の縦の中央。
fn row_center(vp: &Viewport<'_>, item: usize) -> f32 {
    let row = vp.row(item);
    row.y + row.h / 2.0
}
