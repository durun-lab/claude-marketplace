//! Timeline の文字。目盛りの時刻と帯の名前。座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 帯の名前は帯の見えている所の左に寄せ（帯の左端が画面の外に出ても読める）、入らない分は切り詰めて「…」を付ける。
//! 名前を入れる幅が狭い帯は測らずにスキップするので、全体表示で数千のカットが細いときはシェーピングしない。
//! 置くのは見えている行の名前だけで、字は行の領域の中だけに描く（目盛りの帯の下に隠れた行の字は描かない）。

use std::sync::Arc;

use crate::geom::{Color, Rect};
use crate::header;
use crate::paint::{bar_color, stripe, trimmed, PaintState};
use crate::palette;
use crate::rows::RowHeight;
use crate::ruler::{label, major_step, ticks};
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::thumbs::{cut_rect, name_inset};
use crate::timeline::{Item, TimelineModel};
use crate::timeline_layout::Viewport;

/// 帯の端と名前の間の余白。
pub const LABEL_PAD: f32 = 4.0;
/// 名前を入れる幅がこれより狭い帯は、測らずに名前を出さない。
const MIN_LABEL_PX: f32 = 12.0;
/// 目盛りの帯の上端から時刻の行の上端まで。
const RULER_LABEL_Y: f32 = 3.0;
/// 目盛りの線と時刻の間隔。
const TICK_GAP: f32 = 4.0;
/// 無効なレイヤーの名前の不透明度。帯と同じく薄くする。
const DISABLED_ALPHA: f32 = 0.6;
/// Medium と Large の行で名前を置く、帯の上の部分の高さの上限。下は帯のマーカー、波形、フィルムストリップに空ける。
const NAME_AREA_H: f32 = 20.0;

/// Timeline の見えている所 `vp` に描く文字。目盛りの時刻と、見えている帯の名前。
pub fn timeline_labels(
    model: &TimelineModel,
    vp: &Viewport,
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut placer = Placer {
        vp,
        measure: &mut *measure,
        out: Vec::new(),
    };
    placer.ruler(state.palette.label, font.small);
    placer.bars(model, state, font.base);
    let mut out = placer.out;
    out.extend(header::labels(model, vp, state, font, measure));
    out
}

/// 文字を置いていく途中の状態。
struct Placer<'a> {
    vp: &'a Viewport<'a>,
    measure: &'a mut dyn TextMeasure,
    out: Vec<Label>,
}

impl Placer<'_> {
    /// 主な目盛りの右に時刻を置く。右端の時刻は、目盛りの帯の右端から目盛りとの間隔だけ離して収まるよう左へ寄せる
    /// （右端に接すると、拡大した倍率で最後の字が切れて見える）。
    /// 目盛りが右端より外にある時刻は置かない（寄せると左の時刻に重なる）。
    fn ruler(&mut self, color: Color, size: f32) {
        let r = self.vp.layout.ruler;
        let step = major_step(self.vp.time.px_per_sec);
        for t in ticks(&self.vp.time, r.w) {
            if !t.major || t.x > r.w {
                continue;
            }
            let text = label(t.time, step);
            let right = r.right() - TICK_GAP;
            let x = (r.x + t.x + TICK_GAP).min(right - self.measure.width(&text, size));
            self.out.push(Label {
                text: text.into(),
                x,
                y: r.y + RULER_LABEL_Y,
                size,
                color,
                cut: None,
                clip: r,
                tabular: false,
            });
        }
    }

    /// 見えている行の帯の名前。シーケンスのレイヤーはカットごと（隙間を除く）に置く。
    fn bars(&mut self, model: &TimelineModel, state: &PaintState, size: f32) {
        for i in self.vp.visible_rows() {
            let Some(item) = model.items.get(i) else {
                break;
            };
            let (start, end) = trimmed(item, i, state.preview);
            let full = self.vp.bar(i, start, end);
            let named = name_area(full, self.vp.rows.height(i));
            if item.cuts.is_empty() {
                let color = layer_text(item, stripe(&state.palette, i));
                let rect = self.after_thumb((i, item.thumb.as_ref()), full, named);
                self.name(&item.label, rect, TextStyle { size, color });
                continue;
            }
            let style = TextStyle {
                size,
                color: cut_text(item),
            };
            for cut in item.cuts.iter().filter(|c| !c.gap) {
                // カットはレイヤーの帯の内側だけを描くので、名前もその中に収める。
                let span = cut_rect(self.vp, full, cut);
                let rect = self.after_thumb((i, cut.thumb.as_ref()), span, named);
                self.name(&cut.label, rect, style);
            }
        }
    }

    /// 帯の矩形 `span` の名前の矩形。サムネイルを出す帯はサムネイルの右から始める（サムネイルは見えている所の左に寄る）。
    /// `named` は帯の名前を置く所（縦の位置と高さ）。
    fn after_thumb(&self, thumb: (usize, Option<&Arc<str>>), span: Rect, named: Rect) -> Rect {
        let left = span.x.max(self.vp.layout.row_area.x);
        let left = left + name_inset(self.vp, thumb.0, thumb.1, span);
        Rect::new(left, named.y, span.right() - left, named.h)
    }

    /// 帯 `bar` の行の領域に見えている所に名前を置く。入らなければ切り詰め、1 字と「…」も入らなければ置かない。
    /// 縦の位置は帯の矩形で決め（スクロールで帯が隠れかけても字は動かない）、字は行の領域の中だけに描く。
    fn name(&mut self, text: &Arc<str>, bar: Rect, style: TextStyle) {
        let area = self.vp.layout.row_area;
        let left = bar.x.max(area.x);
        let visible = Rect::new(left, bar.y, bar.right().min(area.right()) - left, bar.h);
        let (top, bottom) = (visible.y.max(area.y), visible.bottom().min(area.bottom()));
        if text.is_empty() || visible.w - 2.0 * LABEL_PAD < MIN_LABEL_PX || bottom <= top {
            return;
        }
        let placed = place(text, visible, LABEL_PAD, Align::Left, style, self.measure);
        if let Some(mut label) = placed {
            label.clip = Rect::new(visible.x, top, visible.w, bottom - top);
            self.out.push(label);
        }
    }
}

/// 名前を置く帯の部分。Small の行は帯の全体。Medium と Large の行は上の部分で、下端の帯のマーカーに重ねない
/// （Medium は帯の高さ 25 から 9 を除いた 16 に、高さ 15 の 1 行が入る）。
fn name_area(bar: Rect, height: RowHeight) -> Rect {
    match height {
        RowHeight::Small => bar,
        RowHeight::Medium | RowHeight::Large => {
            let h = NAME_AREA_H.min(bar.h - crate::keymarks::MARK_PX).max(0.0);
            Rect::new(bar.x, bar.y, bar.w, h)
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
    use crate::paint::tests::state;
    use crate::palette::tests::KINDS;
    use crate::palette::{contrast, over, Palette};
    use crate::text::line_height;
    use crate::text::tests::FakeMeasure;
    use crate::time_view::TimeView;
    use crate::timeline::{CutBar, ItemKind};
    use crate::timeline_layout::tests::TestViewport;
    use crate::timeline_layout::{HEADER_W, RULER_H, TOOLBAR_H};

    /// 幅 800、高さ 200 の Timeline。
    const SIZE: (f32, f32) = (800.0, 200.0);

    fn labels_in(
        model: &TimelineModel,
        f: &TestViewport,
        state: &PaintState,
    ) -> (Vec<Label>, FakeMeasure) {
        let mut m = FakeMeasure::default();
        let font = UiFont::default();
        let out = timeline_labels(model, &f.vp(), state, &font, &mut m);
        (out, m)
    }

    fn labels_of(model: &TimelineModel, state: &PaintState) -> (Vec<Label>, FakeMeasure) {
        labels_in(model, &TestViewport::new(model, SIZE), state)
    }

    /// 帯の名前（字を描く矩形が行の領域にあるもの）。
    fn bar_labels(out: &[Label]) -> Vec<&Label> {
        out.iter()
            .filter(|l| l.clip.y >= TOOLBAR_H + RULER_H && l.clip.x >= HEADER_W)
            .collect()
    }

    fn model(items: Vec<Item>) -> TimelineModel {
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items,
            ..TimelineModel::default()
        }
    }

    #[test]
    fn 帯の名前は余白を空けて帯の左に置き_入れば切らず_帯の外に描かない() {
        let m = model(vec![
            Item::layer(1.0, 4.0, ItemKind::Footage, true).with_label("a.mp4")
        ]);
        let (out, _) = labels_of(&m, &state(0.0));
        let bars = bar_labels(&out);
        assert_eq!(bars.len(), 1);
        let l = bars[0];
        assert_eq!((l.x, l.cut), (HEADER_W + 100.0 + LABEL_PAD, None));
        assert_eq!(&*l.text, "a.mp4");
        let bar = TestViewport::new(&m, SIZE).vp().bar(0, 1.0, 4.0);
        let area = Rect::new(bar.x, bar.y, bar.w, bar.h - crate::keymarks::MARK_PX);
        assert_eq!(l.clip, area);
        // Medium の行は帯の上の部分の中央に置き、下端の帯のマーカーに重ねない。
        assert_eq!(l.y + line_height(l.size) / 2.0, area.y + area.h / 2.0);
        assert!(l.y + line_height(l.size) <= area.bottom(), "{l:?}");
        assert_eq!(l.color, palette::BAR_TEXT_LIGHT);
    }

    #[test]
    fn large_の行は名前を帯の上の部分に置く() {
        let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true);
        seq.cuts = vec![CutBar {
            start: 0.0,
            end: 6.0,
            label: "a.mp4".into(),
            ..CutBar::default()
        }];
        let m = model(vec![seq]);
        let (out, _) = labels_of(&m, &state(0.0));
        let l = bar_labels(&out)[0];
        let bar = TestViewport::new(&m, SIZE).vp().bar(0, 0.0, 6.0);
        assert_eq!(bar.h, 53.0, "sequence の行は Large");
        assert_eq!(l.y + line_height(l.size) / 2.0, bar.y + NAME_AREA_H / 2.0);
    }

    #[test]
    fn 目盛りの帯の下に隠れかけた行の名前は動かさずに行の領域で切り_隠れた行は置かない() {
        let items = (0..10)
            .map(|i| Item::layer(0.0, 5.0, ItemKind::Image, true).with_label(&format!("row {i}")))
            .collect();
        let m = model(items);
        let mut f = TestViewport::new(&m, SIZE);
        f.scroll_y = 28.0 + 10.0;
        let (out, _) = labels_in(&m, &f, &state(0.0));
        let names: Vec<&str> = bar_labels(&out).iter().map(|l| &*l.text).collect();
        assert!(!names.contains(&"row 0"), "{names:?}");
        let row1 = bar_labels(&out)
            .into_iter()
            .find(|l| &*l.text == "row 1")
            .expect("一部が見えている行");
        let bar = f.vp().bar(1, 0.0, 5.0);
        let area_h = bar.h - crate::keymarks::MARK_PX;
        assert_eq!(row1.y + line_height(row1.size) / 2.0, bar.y + area_h / 2.0);
        assert_eq!(row1.clip.y, f.layout.row_area.y, "上は行の領域で切る");
    }

    #[test]
    fn 入らない名前は帯の幅で切り詰める() {
        // 帯は 0.5 秒（50 px）。字は 1 字 6 px、省略記号は 12 px。名前を入れる幅は 50 - 8 = 42。
        let m = model(vec![
            Item::layer(0.0, 0.5, ItemKind::Text, true).with_label("abcdefghij")
        ]);
        let (out, _) = labels_of(&m, &state(0.0));
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
        let (out, measure) = labels_of(&m, &state(0.0));
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
        let mut f = TestViewport::new(&m, SIZE);
        f.time = TimeView {
            origin: 2.0,
            px_per_sec: 100.0,
        };
        let (out, _) = labels_in(&m, &f, &state(0.0));
        assert_eq!(bar_labels(&out)[0].x, HEADER_W + LABEL_PAD);
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
        let (out, _) = labels_of(&model(vec![seq]), &state(0.0));
        let names: Vec<(&str, f32)> = bar_labels(&out).iter().map(|l| (&*l.text, l.x)).collect();
        assert_eq!(
            names,
            [
                ("a.mp4", HEADER_W + LABEL_PAD),
                ("b.mp4", HEADER_W + 300.0 + LABEL_PAD)
            ]
        );
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
            ..state(0.0)
        };
        let (out, _) = labels_of(&m, &st);
        assert_eq!(bar_labels(&out)[0].x, HEADER_W + 200.0 + LABEL_PAD);
    }

    #[test]
    fn 無効なレイヤーの名前は薄くする() {
        let m = model(vec![
            Item::layer(0.0, 4.0, ItemKind::Footage, false).with_label("off")
        ]);
        let (out, _) = labels_of(&m, &state(0.0));
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
        let (out, measure) = labels_of(&model(items), &state(0.0));
        let rows = bar_labels(&out).len();
        assert!(rows > 0 && rows < 10, "高さ 200 に入る行だけ: {rows}");
        assert!(measure.asked.len() < 60, "{}", measure.asked.len());
    }

    #[test]
    fn 目盛りの時刻は主な目盛りの右に置き_右端に収める() {
        let (out, _) = labels_of(&model(Vec::new()), &state(0.0));
        let ruler: Vec<&Label> = out
            .iter()
            .filter(|l| l.y < TOOLBAR_H + RULER_H && l.x >= HEADER_W)
            .collect();
        assert_eq!(&*ruler[0].text, "0:00");
        assert_eq!(ruler[0].x, HEADER_W + TICK_GAP);
        assert_eq!(ruler[0].color, Palette::DEFAULT.label);
        // 幅 800 で目盛りの帯は 620 px、最後の主な目盛りは 6 秒（600 px）。時刻は右端から間隔を空けて収める。
        let last = ruler.last().expect("時刻");
        assert_eq!(&*last.text, "0:06");
        assert_eq!(last.x, 800.0 - TICK_GAP - 4.0 * 5.5, "4 字 × 5.5 px");
        assert_eq!(
            last.clip,
            Rect::new(HEADER_W, TOOLBAR_H, 800.0 - HEADER_W, RULER_H)
        );
    }

    #[test]
    fn 目盛りが右端より外にある時刻は置かない() {
        // 幅 779 では目盛りの帯が 599 px で、6 秒の目盛り（600 px）が外にある。
        let m = model(Vec::new());
        let f = TestViewport::new(&m, (779.0, 200.0));
        let (out, _) = labels_in(&m, &f, &state(0.0));
        let last = out
            .iter()
            .rfind(|l| l.y < TOOLBAR_H + RULER_H && l.x >= HEADER_W)
            .expect("時刻");
        assert_eq!(&*last.text, "0:05");
    }
}
