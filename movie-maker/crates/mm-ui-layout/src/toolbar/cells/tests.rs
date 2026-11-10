use super::*;
use crate::text::tests::FakeMeasure;
use crate::text::UiFont;

/// 全部の名前。左から並べる順。
const ALL_NAMES: [&str; 10] = [
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

/// テスト用の測り方（英字 1 字が字の大きさの半分の 6）で測った名前の幅。
fn names() -> ToolbarNames {
    ToolbarNames::measure(&UiFont::default(), &mut FakeMeasure::default())
}

fn bar(w: f32) -> Rect {
    Rect::new(0.0, 0.0, w, 30.0)
}

/// 名前を出したボタンの名前。
fn named(c: &Cells) -> Vec<&'static str> {
    c.buttons
        .iter()
        .filter(|b| b.named)
        .map(|b| b.spec.name)
        .collect()
}

/// 名前を `dropped` 個のまとまりで省いたとき、全部のボタンとスライダーの下限がちょうど入るツールバーの幅。
fn width_for(n: &ToolbarNames, dropped: usize) -> f32 {
    let p = Plan {
        dropped,
        icon_w: ICON_BUTTON_W,
        slider_w: 0.0,
    };
    let buttons: f32 = (0..BUTTONS.len()).map(|i| p.button(i, n).0).sum();
    buttons + gaps() + BUTTON_GAP + SLIDER_MIN_W + 2.0 * PAD
}

#[test]
fn ボタンは左から_undo_からスナップの順に並び_右にズームのまとまりが続く() {
    let b = bar(1000.0);
    let c = cells(b, &names());
    let order: Vec<_> = c.buttons.iter().map(|b| b.spec.name).collect();
    assert_eq!(
        order,
        [
            "Undo",
            "Redo",
            "Selection",
            "Trim",
            "Blade",
            "Split",
            "Lift",
            "Extract",
            "Snapping",
            "",
            "",
            "Fit"
        ]
    );
    let mut right = 0.0;
    for cell in &c.buttons {
        assert!(
            cell.rect.x >= right,
            "{}: 重ならず左から右へ",
            cell.spec.name
        );
        right = cell.rect.right();
        assert_eq!(cell.rect.h, BUTTON_H);
        assert_eq!(cell.rect.y + cell.rect.h / 2.0, b.h / 2.0, "縦は中央");
    }
    assert!(right <= b.right() - PAD + 0.01, "右端の余白の内側");
    let slider = c.slider.expect("広ければスライダーを出す");
    let (out, zin) = (c.buttons[9].rect, c.buttons[10].rect);
    assert!(out.right() <= slider.rect.x && slider.rect.right() <= zin.x);
    assert_eq!(slider.rect.w, SLIDER_W);
    assert_eq!(c.separators.len(), 4, "ツール、編集、スナップ、ズームの前");
}

#[test]
fn 名前の幅は測った幅で_全部が入れば全部のボタンに名前を出す() {
    let n = names();
    let c = cells(bar(width_for(&n, 0) + 0.5), &n);
    assert_eq!(named(&c), ALL_NAMES);
    let extract = c.buttons.iter().find(|b| b.spec.name == "Extract").unwrap();
    assert_eq!(
        extract.rect.w,
        NAME_PAD * 2.0 + ICON_W + NAME_GAP + 7.0 * 6.0,
        "7 字 × 6"
    );
    let area = extract.name_box().expect("名前の欄");
    assert!(
        area.w >= 7.0 * 6.0 + NAME_PAD - 0.01,
        "右の余白まで含み、測った幅が入る"
    );
    assert!(area.right() <= extract.rect.right());
    let fit = c.buttons.iter().find(|b| b.spec.name == "Fit").unwrap();
    assert!(fit.named);
}

#[test]
fn 狭くなると取り消し_ズーム_ツール_スナップ_編集の順にまとまりごとに名前を省く() {
    let n = names();
    let expect: [&[&str]; 6] = [
        &ALL_NAMES,
        &[
            "Selection",
            "Trim",
            "Blade",
            "Split",
            "Lift",
            "Extract",
            "Snapping",
            "Fit",
        ],
        &[
            "Selection",
            "Trim",
            "Blade",
            "Split",
            "Lift",
            "Extract",
            "Snapping",
        ],
        &["Split", "Lift", "Extract", "Snapping"],
        &["Split", "Lift", "Extract"],
        &[],
    ];
    for (dropped, want) in expect.iter().enumerate() {
        let c = cells(bar(width_for(&n, dropped) + 0.5), &n);
        assert_eq!(named(&c), *want, "{dropped} 個のまとまりを省く幅");
        assert_eq!(c.buttons.len(), BUTTONS.len());
        let slider = c.slider.expect("スライダーの下限が入る");
        assert!(
            (slider.rect.w - SLIDER_MIN_W - 0.5).abs() < 0.01,
            "{slider:?}"
        );
        if let Some(next) = expect.get(dropped + 1) {
            let narrower = cells(bar(width_for(&n, dropped) - 1.0), &n);
            assert_eq!(named(&narrower), *next, "1 狭ければ次のまとまりも省く");
        }
    }
}

#[test]
fn 名前の幅を測る前は名前を出さない() {
    let c = cells(bar(2000.0), &ToolbarNames::default());
    assert!(named(&c).is_empty());
    assert!(c.buttons.iter().all(|b| b.rect.w == ICON_BUTTON_W));
}

#[test]
fn アイコンだけでも入らなければスライダーを出さずにボタンを縮め_下限より狭ければ左のボタンを出さない(
) {
    let n = names();
    let icons = cells(bar(width_for(&n, NAME_DROP_ORDER.len()) - 1.0), &n);
    assert!(
        icons.slider.is_none(),
        "スライダーの下限が入らなければ出さない"
    );
    assert!(icons.buttons.iter().all(|b| b.rect.w == ICON_BUTTON_W));
    // 中央の列をしまった窓の幅の下限（サーフェスの幅 400）でも全部のボタンを出す。
    let b = bar(400.0);
    let shrunk = cells(b, &n);
    assert_eq!(shrunk.buttons.len(), BUTTONS.len());
    for cell in &shrunk.buttons {
        assert!(cell.rect.w < ICON_BUTTON_W && cell.rect.w >= MIN_ICON_BUTTON_W);
        assert!(cell.rect.right() <= b.right() - PAD + 0.01);
    }
    let b = bar(300.0);
    let tiny = cells(b, &n);
    assert!(tiny.buttons.len() < BUTTONS.len());
    let zoom: Vec<_> = tiny
        .buttons
        .iter()
        .filter(|c| c.spec.group == Group::Zoom)
        .collect();
    assert_eq!(zoom.len(), 3, "ズームのまとまりは残す");
    let last_left = tiny
        .buttons
        .iter()
        .filter(|c| c.spec.group != Group::Zoom)
        .map(|c| c.rect.right())
        .fold(0.0, f32::max);
    assert!(
        last_left + GROUP_GAP <= zoom[0].rect.x + 0.01,
        "左のボタンは右のまとまりに掛からない"
    );
}
