//! Timeline の文字。目盛りの時刻と帯の名前。座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 帯の名前は帯の見えている所の左に寄せ（帯の左端が画面の外に出ても読める）、入らない分は切り詰めて「…」を付ける。
//! 名前を入れる幅が狭い帯は測らずにスキップするので、全体表示で数千のカットが細いときはシェーピングしない。

use std::sync::Arc;

use crate::geom::{Color, Rect};
use crate::paint::{bar_color, stripe, trimmed, PaintState};
use crate::palette;
use crate::ruler::{label, major_step, ticks};
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::time_view::TimeView;
use crate::timeline::{cut_span, row_top, span, Item, TimelineModel, ROW_H, RULER_H};

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

impl Placer<'_> {
    /// 主な目盛りの右に時刻を置く。右端の時刻は、Timeline の右端から目盛りとの間隔だけ離して収まるよう左へ寄せる
    /// （右端に接すると、拡大した倍率で最後の字が切れて見える）。
    /// 目盛りが右端より外にある時刻は置かない（寄せると左の時刻に重なる）。
    fn ruler(&mut self, color: Color, size: f32) {
        let (step, width) = (major_step(self.view.px_per_sec), self.width);
        let clip = Rect::new(0.0, 0.0, width, RULER_H);
        for t in ticks(self.view, width) {
            if !t.major || t.x > width {
                continue;
            }
            let text = label(t.time, step);
            let right = width - TICK_GAP;
            let x = (t.x + TICK_GAP).min(right - self.measure.width(&text, size));
            self.out.push(Label {
                text: text.into(),
                x,
                y: RULER_LABEL_Y,
                size,
                color,
                cut: None,
                clip,
            });
        }
    }

    /// 高さ `h` の Timeline に見えている行の帯の名前。シーケンスのレイヤーはカットごと（隙間を除く）に置く。
    fn bars(&mut self, model: &TimelineModel, h: f32, state: &PaintState, size: f32) {
        for (i, item) in model.items.iter().enumerate() {
            let top = row_top(i);
            if top >= h {
                break;
            }
            let (start, end) = trimmed(item, i, state.preview);
            let (x, w) = span(self.view, start, end);
            let bar = Rect::new(x, top, w, ROW_H);
            if item.cuts.is_empty() {
                let color = layer_text(item, stripe(&state.palette, i));
                self.name(&item.label, bar, TextStyle { size, color });
                continue;
            }
            let style = TextStyle {
                size,
                color: cut_text(item),
            };
            for cut in item.cuts.iter().filter(|c| !c.gap) {
                // カットはレイヤーの帯の内側だけを描くので、名前もその中に収める。
                let (cx, cw) = cut_span(self.view, cut);
                let left = cx.max(bar.x);
                let right = (cx + cw).min(bar.right());
                self.name(&cut.label, Rect::new(left, top, right - left, ROW_H), style);
            }
        }
    }

    /// 帯 `bar` の見えている所に名前を置く。入らなければ切り詰め、1 字と「…」も入らなければ置かない。
    fn name(&mut self, text: &Arc<str>, bar: Rect, style: TextStyle) {
        let left = bar.x.max(0.0);
        let visible = Rect::new(left, bar.y, bar.right().min(self.width) - left, bar.h);
        if text.is_empty() || visible.w - 2.0 * LABEL_PAD < MIN_LABEL_PX {
            return;
        }
        if let Some(label) = place(text, visible, LABEL_PAD, Align::Left, style, self.measure) {
            self.out.push(label);
        }
    }
}

/// レイヤーの帯の名前の色。帯を描いた色（無効なレイヤーは行の背景 `behind` に薄く重ねた色）の上で読める字にし、
/// 無効なレイヤーは字も薄くする。
fn layer_text(item: &Item, behind: Color) -> Color {
    let color = palette::text_on(palette::over(bar_color(item), behind));
    if item.enabled {
        color
    } else {
        color.with_alpha(DISABLED_ALPHA)
    }
}

/// カットの名前の色。カットは無効なレイヤーでも種類の色で塗るので、その色の上で読める字にし、無効なら字を薄くする。
fn cut_text(item: &Item) -> Color {
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
    use crate::palette::tests::KINDS;
    use crate::palette::{contrast, over, Palette};
    use crate::text::line_height;
    use crate::text::tests::FakeMeasure;
    use crate::timeline::{CutBar, Hit, ItemKind};

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
    fn 帯の名前は余白を空けて帯の左に置き_入れば切らず_帯の外に描かない() {
        let m = model(vec![
            Item::layer(1.0, 4.0, ItemKind::Footage, true).with_label("a.mp4")
        ]);
        let (out, _) = labels_of(&m, &state());
        let bars = bar_labels(&out);
        assert_eq!(bars.len(), 1);
        let l = bars[0];
        assert_eq!((l.x, l.cut), (100.0 + LABEL_PAD, None));
        assert_eq!(&*l.text, "a.mp4");
        assert_eq!(l.clip, Rect::new(100.0, row_top(0), 300.0, ROW_H));
        // 行の矩形は帯の高さの中央。
        let line = line_height(l.size);
        assert_eq!(l.y + line / 2.0, row_top(0) + ROW_H / 2.0);
        assert_eq!(l.color, palette::BAR_TEXT_LIGHT);
    }

    #[test]
    fn 入らない名前は帯の幅で切り詰める() {
        // 帯は 0.5 秒（50 px）。字は 1 字 6 px、省略記号は 12 px。名前を入れる幅は 50 - 8 = 42。
        let m = model(vec![
            Item::layer(0.0, 0.5, ItemKind::Text, true).with_label("abcdefghij")
        ]);
        let (out, _) = labels_of(&m, &state());
        let l = bar_labels(&out)[0];
        assert_eq!(
            l.cut,
            Some(30.0),
            "5 字（30 px）と省略記号（12 px）で 42 px"
        );
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
            "25 px の帯は測るが、1 字（12 px）と省略記号（12 px）が 17 px に入らない"
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
        let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true).with_label("s");
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
    fn 無効なレイヤーの名前は行の背景に薄く重ねた帯の上で読める字にする() {
        // 暗い背景（既定とテーマのダーク）と明るい背景（テーマのライト）。
        let behinds = [
            palette::ROW_EVEN,
            palette::ROW_ODD,
            Color::hex(0x171b24),
            Color::hex(0xece9d8),
        ];
        for (kind, behind) in KINDS.into_iter().flat_map(|k| behinds.map(|b| (k, b))) {
            let item = Item::layer(0.0, 1.0, kind, false);
            let bar = over(bar_color(&item), behind);
            let ratio = contrast(over(layer_text(&item, behind), bar), bar);
            assert!(ratio >= 3.0, "{kind:?} の上 {behind:?}: {ratio}");
        }
        let text = Item::layer(0.0, 1.0, ItemKind::Text, false);
        assert_eq!(
            layer_text(&text, palette::ROW_EVEN).with_alpha(1.0),
            palette::BAR_TEXT_LIGHT,
            "橙の帯でも、暗い背景に薄く重ねたら明るい字"
        );
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
        // 幅 800 で最後の主な目盛りは 8 秒（800 px）。時刻は右端から間隔を空けて収める。
        let last = ruler.last().expect("時刻");
        assert_eq!(&*last.text, "0:08");
        assert_eq!(last.x, 800.0 - TICK_GAP - 4.0 * 5.5, "4 字 × 5.5 px");
        assert_eq!(last.clip, Rect::new(0.0, 0.0, 800.0, RULER_H));
    }

    #[test]
    fn 目盛りが右端より外にある時刻は置かない() {
        // 幅 799 では 8 秒の目盛り（800 px）が外にある。
        let mut m = FakeMeasure::default();
        let font = UiFont::default();
        let out = timeline_labels(
            &model(Vec::new()),
            &VIEW,
            (799.0, 200.0),
            &state(),
            &font,
            &mut m,
        );
        let last = out.iter().rfind(|l| l.y < RULER_H).expect("時刻");
        assert_eq!(&*last.text, "0:07");
    }
}
