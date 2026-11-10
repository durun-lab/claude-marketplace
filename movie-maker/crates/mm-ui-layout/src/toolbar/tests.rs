use super::*;
use crate::controller::{Mods, PointerEvent};
use crate::geom::{Quad, QuadShape};
use crate::palette::Palette;
use crate::text::tests::FakeMeasure;

const COMMANDS: [&str; 9] = [
    "edit.undo",
    "edit.redo",
    "clip.split",
    "clip.lift",
    "clip.extract",
    "timeline.toggle_snapping",
    "timeline.zoom_out",
    "timeline.zoom_in",
    "timeline.full_extent_zoom",
];

fn bar(w: f32) -> Rect {
    Rect::new(0.0, 0.0, w, 30.0)
}

/// テスト用の測り方（英字 1 字が 6）で測った名前の幅を持つ状態。
fn state() -> ToolbarState {
    ToolbarState {
        snapping: true,
        px_per_sec: 100.0,
        floor: 2.0,
        names: ToolbarNames::measure(&UiFont::default(), &mut FakeMeasure::default()),
        ..ToolbarState::default()
    }
}

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// 名前 `name` のボタン。
fn cell(b: Rect, s: &ToolbarState, name: &str) -> ButtonCell {
    *cells(b, &s.names)
        .buttons
        .iter()
        .find(|x| x.spec.name == name)
        .unwrap()
}

fn down(c: &mut ToolbarController, bar: Rect, s: &ToolbarState, at: (f32, f32)) {
    let ev = PointerEvent::Down {
        x: at.0,
        y: at.1,
        mods: Mods::default(),
    };
    assert!(c
        .handle(ev, bar, s)
        .iter()
        .all(|a| !matches!(a, ToolbarAction::Command(_))));
}

fn mv(
    c: &mut ToolbarController,
    bar: Rect,
    s: &ToolbarState,
    at: (f32, f32),
) -> Vec<ToolbarAction> {
    c.handle(PointerEvent::Move { x: at.0, y: at.1 }, bar, s)
}

#[test]
fn コマンドのあるボタンは表の_id_で引け_ツールのボタンは場所だけで押せない() {
    let b = bar(1000.0);
    let s = state();
    for id in COMMANDS {
        assert!(toolbar_button(b, &s.names, id).is_some(), "{id}");
    }
    let look = |name: &str| ButtonLook::of(&cell(b, &s, name), &s, &ToolbarController::default());
    assert!(look("Selection").is_lit() && look("Selection").is_enabled());
    assert!(!look("Trim").is_enabled() && !look("Blade").is_enabled());
    let ctl = &mut ToolbarController::default();
    down(ctl, b, &s, center(cell(b, &s, "Trim").rect));
    assert_eq!(ctl.pressed(), None, "コマンドの無いボタンは押せない");
    assert_eq!(ctl.hover(), None);
}

#[test]
fn 押せないコマンドは薄く_押せず_snapping_は入のとき押された見た目() {
    let b = bar(1000.0);
    let mut s = state();
    s.disabled = vec!["clip.lift"];
    let ctl = ToolbarController::default();
    let lift = ButtonLook::of(&cell(b, &s, "Lift"), &s, &ctl);
    assert!(!lift.is_enabled());
    let p = Palette::DEFAULT;
    assert!(lift.ink(&p).0[3] < p.label.0[3]);
    assert!(lift.back(&p).is_none());
    let snap = ButtonLook::of(&cell(b, &s, "Snapping"), &s, &ctl);
    assert!(snap.is_lit());
    assert_eq!(snap.back(&p), Some(p.accent));
    s.snapping = false;
    let off = ButtonLook::of(&cell(b, &s, "Snapping"), &s, &ctl);
    assert!(!off.is_lit() && off.back(&p).is_none());
    let mut c2 = ToolbarController::default();
    down(&mut c2, b, &s, center(cell(b, &s, "Lift").rect));
    assert_eq!(c2.pressed(), None, "押せないボタンは押せない");
}

#[test]
fn ボタンは同じボタンの上で離したときに実行し_外で離せば取りやめ_ホバーで明るく押している間は押した見た目(
) {
    let b = bar(1000.0);
    let s = state();
    let undo = cell(b, &s, "Undo");
    let redo = cell(b, &s, "Redo").rect;
    let p = Palette::DEFAULT;
    let mut ctl = ToolbarController::default();
    mv(&mut ctl, b, &s, center(undo.rect));
    let hover = ButtonLook::of(&undo, &s, &ctl);
    assert!(
        hover.back(&p).is_some() && !hover.is_pressed(),
        "乗れば明るく"
    );
    down(&mut ctl, b, &s, center(undo.rect));
    let held = ButtonLook::of(&undo, &s, &ctl);
    assert!(held.is_pressed());
    assert_eq!(
        held.place(undo.rect),
        undo.rect.translated(1.0, 1.0),
        "押した見た目は右下へずらす"
    );
    assert_eq!(
        ctl.handle(PointerEvent::Up, b, &s),
        [ToolbarAction::Command("edit.undo")]
    );
    down(&mut ctl, b, &s, center(undo.rect));
    mv(&mut ctl, b, &s, center(redo));
    assert!(
        !ButtonLook::of(&undo, &s, &ctl).is_pressed(),
        "外へずらせば押した見た目を外す"
    );
    assert!(
        ctl.handle(PointerEvent::Up, b, &s).is_empty(),
        "別のボタンの上で離しても実行しない"
    );
    down(&mut ctl, b, &s, center(undo.rect));
    mv(&mut ctl, b, &s, center(undo.rect));
    assert_eq!(
        ctl.handle(PointerEvent::Up, b, &s).len(),
        1,
        "戻せば実行する"
    );
    down(&mut ctl, b, &s, center(undo.rect));
    ctl.handle(PointerEvent::Cancel, b, &s);
    assert!(ctl.handle(PointerEvent::Up, b, &s).is_empty());
}

#[test]
fn 押したあと押せなくなったボタンは離しても実行しない() {
    let b = bar(1000.0);
    let mut s = state();
    let split = toolbar_button(b, &s.names, "clip.split").unwrap();
    let mut ctl = ToolbarController::default();
    down(&mut ctl, b, &s, center(split));
    s.disabled = vec!["clip.split"];
    assert!(ctl.handle(PointerEvent::Up, b, &s).is_empty());
}

#[test]
fn 名前は測った幅のまま切らずに置き_字のアイコンは記号のフォントが無ければ標準の記号の字にする() {
    let b = bar(1000.0);
    let mut s = state();
    s.symbols = true;
    let font = UiFont::default();
    let ctl = ToolbarController::default();
    let p = Palette::DEFAULT;
    let labels =
        |s: &ToolbarState| toolbar_labels(b, s, &ctl, &p, &font, &mut FakeMeasure::default());
    let with = labels(&s);
    let texts: Vec<&str> = with.iter().map(|l| &*l.text).collect();
    for name in ["Undo", "Selection", "Extract", "Snapping", "Fit"] {
        let l = with.iter().find(|l| &*l.text == name).expect(name);
        assert_eq!(l.cut, None, "{name} は切らない");
    }
    assert!(
        texts.contains(&"\u{E7A7}") && texts.contains(&"\u{E8C6}"),
        "Undo と Split は記号のフォントの字"
    );
    s.symbols = false;
    let plain: Vec<String> = labels(&s).iter().map(|l| l.text.to_string()).collect();
    assert!(plain
        .iter()
        .all(|t| !t.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c))));
    assert!(
        plain.contains(&"\u{21B6}".to_owned()),
        "Undo は標準の記号の字"
    );
}

#[test]
fn lift_は抜いた所に隙間を残し_extract_は両側が内向きに寄って隙間を詰める形() {
    let color = Palette::DEFAULT.label;
    let lift = icon_quads(Icon::Lift, (0.0, 0.0), color);
    let extract = icon_quads(Icon::Extract, (0.0, 0.0), color);
    for icon in [&lift, &extract] {
        assert!(
            icon.iter().any(|q| q.shape == QuadShape::TriUp),
            "上向きの矢印"
        );
    }
    let track = |q: &&Quad| q.shape == QuadShape::Rect && q.rect.y >= 4.0;
    assert!(
        lift.iter()
            .filter(track)
            .all(|q| q.rect.x >= 3.0 || q.rect.right() <= -3.0),
        "中ほどは空く"
    );
    let right = extract
        .iter()
        .find(|q| q.shape == QuadShape::TriRight)
        .expect("左から寄る");
    let left = extract
        .iter()
        .find(|q| q.shape == QuadShape::TriLeft)
        .expect("右から寄る");
    assert_eq!(
        (right.rect.right(), left.rect.x),
        (0.0, 0.0),
        "中ほどで突き合わせる"
    );
}

#[test]
fn スライダーの位置は秒あたりのピクセルの対数に比例し_下限が_0_上限が_1() {
    let floor = 2.0;
    assert_eq!(zoom_fraction(floor, floor), 0.0);
    assert_eq!(zoom_fraction(MAX_PX_PER_SEC, floor), 1.0);
    assert_eq!(zoom_fraction(0.1, floor), 0.0, "下限より細かければ 0");
    let mid = (floor * MAX_PX_PER_SEC).sqrt();
    assert!(
        (zoom_fraction(mid, floor) - 0.5).abs() < 1e-6,
        "相乗平均が中央"
    );
    for f in [0.0, 0.1, 0.5, 0.9, 1.0] {
        let back = zoom_fraction(zoom_px(f, floor), floor);
        assert!((back - f).abs() < 1e-5, "{f} -> {back}");
    }
    assert!((zoom_px(1.0, floor) - MAX_PX_PER_SEC).abs() < 1e-6);
    assert!((zoom_px(0.0, floor) - floor).abs() < 1e-9);
    let long = 0.05;
    assert!(
        (zoom_px(0.0, long) - long).abs() < 1e-12,
        "長いコンポジションの下限は MIN_PX_PER_SEC より細かい"
    );
}

#[test]
fn スライダーのドラッグは押したときと動くたびに倍率を返し_溝の外は端に寄せ_離せば止まる() {
    let b = bar(1000.0);
    let s = state();
    let slider = cells(b, &s.names).slider.unwrap();
    let y = slider.rect.y + slider.rect.h / 2.0;
    let mut ctl = ToolbarController::default();
    let at = PointerEvent::Down {
        x: slider.track.x,
        y,
        mods: Mods::default(),
    };
    assert_eq!(ctl.handle(at, b, &s), [ToolbarAction::Zoom(s.floor)]);
    assert!(ctl.dragging());
    let mid = mv(&mut ctl, b, &s, (slider.track.x + slider.track.w / 2.0, y));
    let [ToolbarAction::Zoom(px)] = mid[..] else {
        panic!("{mid:?}");
    };
    assert!((px - (s.floor * MAX_PX_PER_SEC).sqrt()).abs() < 1e-6);
    let beyond = mv(&mut ctl, b, &s, (b.right() + 500.0, y + 100.0));
    assert_eq!(
        beyond,
        [ToolbarAction::Zoom(MAX_PX_PER_SEC)],
        "溝の外へ出ても端で追う"
    );
    ctl.handle(PointerEvent::Up, b, &s);
    assert!(!ctl.dragging());
    assert!(
        mv(&mut ctl, b, &s, (slider.track.x, y)).is_empty(),
        "離した後は動かさない"
    );
}
