use mm_ui_layout::palette::Palette;
use mm_ui_layout::{
    toolbar_labels, Label, Rect, TextMeasure, ToolbarController, ToolbarNames, ToolbarState, UiFont,
};

use crate::fonts::Shaper;

/// 全部のボタンの名前。左から並べる順。
const NAMES: [&str; 10] = [
    "Undo",
    "Redo",
    "Selection",
    "Trim",
    "Blade",
    "Split",
    "Lift",
    "Extract",
    "Snapping",
    "Fit",
];

/// 幅 `width` のツールバーの文字を、OS のフォントで測って置く。`symbols` が偽なら記号のフォントが無いものとして置く。
fn labels_at(s: &mut Shaper, width: f32, symbols: bool) -> Vec<Label> {
    let font = UiFont::default();
    let state = ToolbarState {
        names: ToolbarNames::measure(&font, s),
        symbols,
        ..ToolbarState::default()
    };
    let bar = Rect::new(0.0, 0.0, width, 30.0);
    toolbar_labels(
        bar,
        &state,
        &ToolbarController::default(),
        &Palette::DEFAULT,
        &font,
        s,
    )
}

/// 置いた名前。どれも切り詰めていないことを確かめる。
fn names_at(s: &mut Shaper, width: f32) -> Vec<String> {
    let labels = labels_at(s, width, true);
    let names: Vec<&Label> = labels
        .iter()
        .filter(|l| NAMES.contains(&&*l.text))
        .collect();
    for l in &names {
        assert_eq!(l.cut, None, "{} を切り詰めた（幅 {width}）", l.text);
    }
    names.iter().map(|l| l.text.to_string()).collect()
}

#[test]
fn 名前は文字の仕組みで測り_中央の列をしまったサーフェスの幅_940_では全部の名前を出す() {
    let mut s = Shaper::new(&UiFont::default().family);
    assert_eq!(
        names_at(&mut s, 940.0),
        NAMES,
        "1600 の窓から左右の列（260 と 400）を除いた幅"
    );
    let now = names_at(&mut s, 800.0);
    for name in ["Split", "Lift", "Extract", "Snapping"] {
        assert!(
            now.contains(&name.to_owned()),
            "1600 の窓の半分の幅でも {name} の名前は出す: {now:?}"
        );
    }
    assert!(
        !now.contains(&"Undo".to_owned()),
        "取り消しの名前から先に省く: {now:?}"
    );
}

#[test]
fn 字のアイコンは記号のフォントにも_無いときの標準の記号の字にもあり_豆腐にしない() {
    let mut s = Shaper::new(&UiFont::default().family);
    let symbols = s.has_symbol_font();
    for use_symbols in [true, false].into_iter().filter(|u| symbols || !u) {
        let icons: Vec<Label> = labels_at(&mut s, 940.0, use_symbols)
            .into_iter()
            .filter(|l| l.text.chars().count() == 1)
            .collect();
        assert_eq!(icons.len(), 6, "Undo、Redo、Split、Zoom Out、Zoom In、Fit");
        for icon in &icons {
            s.stops(&icon.text, icon.size);
            let (fonts, cache) = s.split();
            let shaped = cache.get(&icon.text, icon.size).expect("シェーピングした");
            let run = shaped.buffer.layout_runs().next().expect("1 行");
            let glyph = &run.glyphs[0];
            assert_ne!(
                glyph.glyph_id, 0,
                "{:?} がどのフォントにも無い（.notdef）",
                icon.text
            );
            let face = &fonts.db().face(glyph.font_id).expect("フォント").families[0].0;
            if use_symbols {
                assert!(
                    face.starts_with("Segoe"),
                    "{:?} を {face} で描いた",
                    icon.text
                );
            }
        }
    }
}
