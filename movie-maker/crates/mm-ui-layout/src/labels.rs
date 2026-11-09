//! Timeline の文字。目盛りの時刻と帯の名前。座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 帯の名前は帯の見えている所の左に寄せ（帯の左端が画面の外に出ても読める）、入らない分は切り詰めて「…」を付ける。
//! 名前を入れる幅が狭い帯は測らずに飛ばすので、全体表示で数千のカットが細いときは文字を組まない。

use std::sync::Arc;

use crate::geom::{Color, Rect};
use crate::paint::{trimmed, PaintState};
use crate::palette;
use crate::ruler::{label, major_step, ticks};
use crate::text::{fit, line_height, Fit, Label, TextMeasure, UiFont, ELLIPSIS};
use crate::time_view::TimeView;
use crate::timeline::{cut_span, row_top, span, Item, TimelineModel, ROW_H};

/// 帯の端と名前の間の余白。
pub const LABEL_PAD: f32 = 4.0;
/// 名前を入れる幅がこれより狭い帯は、測らずに名前を出さない。
const MIN_LABEL_PX: f32 = 12.0;
/// 目盛りの時刻の行の上端。
const RULER_LABEL_Y: f32 = 3.0;
/// 目盛りの線と時刻の間隔。
const TICK_GAP: f32 = 4.0;
/// 無効なレイヤーの名前の不透明度。帯と同じく薄くする。
const DISABLED_ALPHA: f32 = 0.6;

/// Timeline の矩形の大きさ `(w, h)` の中に描く文字。目盛りの時刻と、見えている帯の名前。
pub fn timeline_labels(
    model: &TimelineModel,
    view: &TimeView,
    size: (f32, f32),
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut placer = Placer {
        view,
        width: size.0,
        measure,
        out: Vec::new(),
    };
    placer.ruler(state.palette.label, font.small);
    placer.bars(model, size.1, state, font.base);
    placer.out
}

/// 文字を置いていく途中の状態。
struct Placer<'a> {
    view: &'a TimeView,
    /// Timeline の幅。
    width: f32,
    measure: &'a mut dyn TextMeasure,
    out: Vec<Label>,
}

/// 帯の名前の置き場所（左右の端、行の上端、字の大きさ、色）。
struct Slot {
    left: f32,
    right: f32,
    y: f32,
    size: f32,
    color: Color,
}

impl Placer<'_> {
    /// 主な目盛りの右に時刻を置く。右端の時刻は Timeline の右端に収まるよう左へ寄せる。
    fn ruler(&mut self, color: Color, size: f32) {
        let step = major_step(self.view.px_per_sec);
        for t in ticks(self.view, self.width).into_iter().filter(|t| t.major) {
            let text = label(t.time, step);
            let x = (t.x + TICK_GAP).min(self.width - self.measure.width(&text, size));
            self.out.push(Label {
                text: text.into(),
                x,
                y: RULER_LABEL_Y,
                size,
                color,
                cut: None,
            });
        }
    }

    /// 高さ `h` の Timeline に見えている行の帯の名前。シーケンスのレイヤーはカットごと（隙間を除く）に置く。
    fn bars(&mut self, model: &TimelineModel, h: f32, state: &PaintState, size: f32) {
        let line = line_height(size);
        for (i, item) in model.items.iter().enumerate() {
            let top = row_top(i);
            if top >= h {
                break;
            }
            let (start, end) = trimmed(item, i, state.preview);
            let (x, w) = span(self.view, start, end);
            let bar = Rect::new(x, top, w, ROW_H);
            let (y, color) = (top + (ROW_H - line) / 2.0, text_color(item));
            if item.cuts.is_empty() {
                let slot = Slot {
                    left: bar.x,
                    right: bar.right(),
                    y,
                    size,
                    color,
                };
                self.name(&item.label, slot);
                continue;
            }
            for cut in item.cuts.iter().filter(|c| !c.gap) {
                // カットはレイヤーの帯の内側だけを描くので、名前もその中に収める。
                let (cx, cw) = cut_span(self.view, cut);
                let slot = Slot {
                    left: cx.max(bar.x),
                    right: (cx + cw).min(bar.right()),
                    y,
                    size,
                    color,
                };
                self.name(&cut.label, slot);
            }
        }
    }

    /// 帯の見えている所に名前を置く。入らなければ切り詰め、1 字も入らなければ置かない。
    fn name(&mut self, text: &Arc<str>, slot: Slot) {
        let left = slot.left.max(0.0) + LABEL_PAD;
        let room = slot.right.min(self.width) - LABEL_PAD - left;
        if text.is_empty() || room < MIN_LABEL_PX {
            return;
        }
        let ellipsis = self.measure.width(ELLIPSIS, slot.size);
        let cut = match fit(self.measure.stops(text, slot.size), ellipsis, room) {
            Fit::Whole => None,
            Fit::Cut(w) => Some(w),
            Fit::Hidden => return,
        };
        self.out.push(Label {
            text: text.clone(),
            x: left,
            y: slot.y,
            size: slot.size,
            color: slot.color,
            cut,
        });
    }
}

/// 帯の名前の色。帯の種類の色の上で読める字にし、無効なレイヤーは帯と同じく薄くする。
fn text_color(item: &Item) -> Color {
    let color = palette::text_on(palette::item_color(item.kind));
    if item.enabled {
        color
    } else {
        color.with_alpha(DISABLED_ALPHA)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::{EditKind, Side};
    use crate::palette::Palette;
    use crate::text::tests::FakeMeasure;
    use crate::timeline::{CutBar, Hit, ItemKind, RULER_H};

    const VIEW: TimeView = TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    };

    fn state() -> PaintState {
        PaintState {
            playhead: 0.0,
            hover: Hit::Empty,
            selected: None,
            selected_cut: None,
            guide: None,
            preview: None,
            palette: Palette::DEFAULT,
        }
    }

    fn labels_of(model: &TimelineModel, state: &PaintState) -> (Vec<Label>, FakeMeasure) {
        let mut m = FakeMeasure::default();
        let font = UiFont::default();
        let out = timeline_labels(model, &VIEW, (800.0, 200.0), state, &font, &mut m);
        (out, m)
    }

    fn bar_labels(out: &[Label]) -> Vec<&Label> {
        out.iter().filter(|l| l.y > RULER_H).collect()
    }

    fn model(items: Vec<Item>) -> TimelineModel {
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items,
        }
    }

    #[test]
    fn 帯の名前は余白を空けて帯の左に置き_入れば切らない() {
        let m = model(vec![
            Item::layer(1.0, 4.0, ItemKind::Footage, true).with_label("Footage: a.mp4")
        ]);
        let (out, _) = labels_of(&m, &state());
        let bars = bar_labels(&out);
        assert_eq!(bars.len(), 1);
        let l = bars[0];
        assert_eq!((l.x, l.cut), (100.0 + LABEL_PAD, None));
        assert_eq!(&*l.text, "Footage: a.mp4");
        // 行の箱は帯の高さの中央。
        let line = line_height(l.size);
        assert_eq!(l.y + line / 2.0, row_top(0) + ROW_H / 2.0);
        assert_eq!(l.color, palette::BAR_TEXT_LIGHT);
    }

    #[test]
    fn 入らない名前は帯の幅で切り詰める() {
        // 帯は 0.5 秒（50 px）。字は 1 字 6 px、三点は 12 px。名前を入れる幅は 50 - 8 = 42。
        let m = model(vec![
            Item::layer(0.0, 0.5, ItemKind::Text, true).with_label("abcdefghij")
        ]);
        let (out, _) = labels_of(&m, &state());
        let l = bar_labels(&out)[0];
        assert_eq!(l.cut, Some(30.0), "5 字（30 px）と三点（12 px）で 42 px");
        assert_eq!(l.color, palette::BAR_TEXT_DARK, "明るい帯には暗い字");
    }

    #[test]
    fn 狭い帯は測らずに出さず_一字も入らない帯も出さない() {
        let m = model(vec![
            Item::layer(0.0, 0.15, ItemKind::Footage, true).with_label("narrow"),
            Item::layer(0.0, 0.25, ItemKind::Footage, true).with_label("日本語"),
        ]);
        let (out, measure) = labels_of(&m, &state());
        assert!(bar_labels(&out).is_empty());
        assert!(
            !measure.asked.iter().any(|t| t == "narrow"),
            "15 px の帯は名前を入れる幅が 7 px なので測らない"
        );
        assert!(
            measure.asked.iter().any(|t| t == "日本語"),
            "25 px の帯は測るが、1 字（12 px）と三点（12 px）が 17 px に入らない"
        );
    }

    #[test]
    fn 左が画面の外に出た帯は見えている所の左に名前を寄せる() {
        let m = model(vec![
            Item::layer(0.0, 5.0, ItemKind::Footage, true).with_label("long take")
        ]);
        let view = TimeView {
            origin: 2.0,
            px_per_sec: 100.0,
        };
        let mut measure = FakeMeasure::default();
        let font = UiFont::default();
        let out = timeline_labels(&m, &view, (800.0, 200.0), &state(), &font, &mut measure);
        assert_eq!(bar_labels(&out)[0].x, LABEL_PAD);
    }

    #[test]
    fn シーケンスはカットごとに名前を置き_隙間には置かない() {
        let cut = |start, end, gap, label: &str| CutBar {
            start,
            end,
            gap,
            label: label.into(),
            ..CutBar::default()
        };
        let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true).with_label("Sequence: s");
        seq.cuts = vec![
            cut(0.0, 2.0, false, "a.mp4"),
            cut(2.0, 3.0, true, "gap"),
            cut(3.0, 6.0, false, "b.mp4"),
        ];
        let (out, _) = labels_of(&model(vec![seq]), &state());
        let names: Vec<(&str, f32)> = bar_labels(&out).iter().map(|l| (&*l.text, l.x)).collect();
        assert_eq!(names, [("a.mp4", LABEL_PAD), ("b.mp4", 300.0 + LABEL_PAD)]);
    }

    #[test]
    fn ドラッグ中の_trim_layer_は動かした後の帯に名前を置く() {
        let m = model(vec![
            Item::layer(1.0, 4.0, ItemKind::Footage, true).with_label("x")
        ]);
        let st = PaintState {
            preview: Some((
                EditKind::TrimLayer {
                    item: 0,
                    side: Side::In,
                },
                1.0,
            )),
            ..state()
        };
        let (out, _) = labels_of(&m, &st);
        assert_eq!(bar_labels(&out)[0].x, 200.0 + LABEL_PAD);
    }

    #[test]
    fn 無効なレイヤーの名前は薄くする() {
        let m = model(vec![
            Item::layer(0.0, 4.0, ItemKind::Footage, false).with_label("off")
        ]);
        let (out, _) = labels_of(&m, &state());
        let c: Color = bar_labels(&out)[0].color;
        assert_eq!(c.0[3], DISABLED_ALPHA);
    }

    #[test]
    fn 見えない行の名前は測らない() {
        let items = (0..1000)
            .map(|i| Item::layer(0.0, 5.0, ItemKind::Image, true).with_label(&format!("row {i}")))
            .collect();
        let (out, measure) = labels_of(&model(items), &state());
        let rows = bar_labels(&out).len();
        assert!(rows > 0 && rows < 10, "高さ 200 に入る行だけ: {rows}");
        assert!(measure.asked.len() < 30, "{}", measure.asked.len());
    }

    #[test]
    fn 目盛りの時刻は主な目盛りの右に置き_右端に収める() {
        let (out, _) = labels_of(&model(Vec::new()), &state());
        let ruler: Vec<&Label> = out.iter().filter(|l| l.y < RULER_H).collect();
        assert_eq!(&*ruler[0].text, "0:00");
        assert_eq!(ruler[0].x, TICK_GAP);
        assert_eq!(ruler[0].color, Palette::DEFAULT.label);
        // 幅 800 で最後の主な目盛りは 8 秒（800 px）。時刻は右端に収める。
        let last = ruler.last().expect("時刻");
        assert_eq!(&*last.text, "0:08");
        assert_eq!(last.x, 800.0 - 4.0 * 5.5, "4 字 × 5.5 px");
    }
}
