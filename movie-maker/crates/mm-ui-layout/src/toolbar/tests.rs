use super::*;
use crate::controller::{Mods, PointerEvent};
use crate::palette::Palette;

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

fn state() -> ToolbarState {
    ToolbarState {
        snapping: true,
        px_per_sec: 100.0,
        floor: 2.0,
        ..ToolbarState::default()
    }
}

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
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
fn ボタンは左から_undo_からスナップの順に並び_右にズームのグループが続く() {
    let b = bar(1000.0);
    let c = cells(b);
    let names: Vec<_> = c.buttons.iter().map(|b| b.spec.name).collect();
    assert_eq!(
        names,
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
        assert!(cell.rect.y >= b.y && cell.rect.bottom() <= b.bottom());
        assert_eq!(cell.rect.h, BUTTON_H);
        assert_eq!(cell.rect.y + cell.rect.h / 2.0, b.h / 2.0, "縦は中央");
    }
    assert!(right <= b.right() - PAD + 0.01, "右端の余白の内側");
    let slider = c.slider.expect("広ければスライダーを出す");
    let out = toolbar_button(b, "timeline.zoom_out").expect("Zoom Out");
    let zin = toolbar_button(b, "timeline.zoom_in").expect("Zoom In");
    assert!(out.right() <= slider.rect.x && slider.rect.right() <= zin.x);
    assert_eq!(slider.rect.w, SLIDER_W);
}

#[test]
fn 広ければ名前を出し_狭ければアイコンだけにして_さらに狭ければスライダーを出さない() {
    let named = cells(bar(1000.0));
    assert!(named.buttons.iter().all(|b| b.named));
    let narrow_w = left_width(true) + GROUP_GAP + zoom_buttons_width(true) + 40.0;
    let narrow = cells(bar(narrow_w));
    assert!(narrow
        .buttons
        .iter()
        .all(|b| !b.named && b.rect.w == ICON_BUTTON_W));
    assert!(narrow.buttons.iter().all(|b| b.name_box().is_none()));
    assert!(
        narrow.slider.is_some(),
        "アイコンなら入る幅にスライダーも出す"
    );
    let tiny = cells(bar(left_width(false)
        + GROUP_GAP
        + zoom_buttons_width(false)
        + 20.0));
    assert!(tiny.slider.is_none());
    assert_eq!(tiny.buttons.len(), 12, "ツールの 3 つを含む全部");
}

#[test]
fn 幅が足りなければ右のズームのグループに掛かる左のボタンを出さない() {
    let b = bar(300.0);
    let c = cells(b);
    let zoom = toolbar_button(b, "timeline.zoom_out").expect("ズームは残す");
    let last_left = c
        .buttons
        .iter()
        .filter(|x| {
            !x.spec.command.is_some_and(|id| {
                id.starts_with("timeline.zoom") || id.ends_with("full_extent_zoom")
            })
        })
        .map(|x| x.rect.right())
        .fold(0.0, f32::max);
    assert!(last_left <= zoom.x, "{last_left} <= {}", zoom.x);
    assert!(c.buttons.iter().all(|x| x.rect.right() <= b.right()));
}

#[test]
fn コマンドのあるボタンは表の_id_で引け_ツールのボタンは場所だけで押せない() {
    let b = bar(1000.0);
    for id in COMMANDS {
        assert!(toolbar_button(b, id).is_some(), "{id}");
    }
    let s = state();
    let c = cells(b);
    let look = |name: &str| {
        let cell = c.buttons.iter().find(|x| x.spec.name == name).unwrap();
        ButtonLook::of(cell, &s, &ToolbarController::default())
    };
    assert!(look("Selection").is_lit() && look("Selection").is_enabled());
    assert!(!look("Trim").is_enabled() && !look("Blade").is_enabled());
    let ctl = &mut ToolbarController::default();
    let trim = c
        .buttons
        .iter()
        .find(|x| x.spec.name == "Trim")
        .unwrap()
        .rect;
    down(ctl, b, &s, center(trim));
    assert_eq!(ctl.pressed(), None, "コマンドの無いボタンは押せない");
    assert_eq!(ctl.hover(), None);
}

#[test]
fn 押せないコマンドは薄く_押せず_snapping_は入のとき押された見た目() {
    let b = bar(1000.0);
    let mut s = state();
    s.disabled = vec!["clip.lift"];
    let c = cells(b);
    let find = |name: &str| *c.buttons.iter().find(|x| x.spec.name == name).unwrap();
    let ctl = ToolbarController::default();
    let lift = ButtonLook::of(&find("Lift"), &s, &ctl);
    assert!(!lift.is_enabled());
    let p = Palette::DEFAULT;
    assert!(lift.ink(&p).0[3] < p.label.0[3]);
    assert!(lift.back(&p).is_none());
    let snap = ButtonLook::of(&find("Snapping"), &s, &ctl);
    assert!(snap.is_lit());
    assert_eq!(snap.back(&p), Some(p.accent));
    s.snapping = false;
    let off = ButtonLook::of(&find("Snapping"), &s, &ctl);
    assert!(!off.is_lit() && off.back(&p).is_none());
    let mut c2 = ToolbarController::default();
    down(&mut c2, b, &s, center(find("Lift").rect));
    assert_eq!(c2.pressed(), None, "押せないボタンは押せない");
}

#[test]
fn ボタンは同じボタンの上で離したときに実行し_外で離せば取りやめ_ホバーで明るく押している間は押した見た目(
) {
    let b = bar(1000.0);
    let s = state();
    let undo = toolbar_button(b, "edit.undo").unwrap();
    let redo = toolbar_button(b, "edit.redo").unwrap();
    let p = Palette::DEFAULT;
    let mut ctl = ToolbarController::default();
    mv(&mut ctl, b, &s, center(undo));
    let cell = *cells(b).buttons.iter().find(|x| x.rect == undo).unwrap();
    let hover = ButtonLook::of(&cell, &s, &ctl);
    assert!(
        hover.back(&p).is_some() && !hover.is_pressed(),
        "乗れば明るく"
    );
    down(&mut ctl, b, &s, center(undo));
    let held = ButtonLook::of(&cell, &s, &ctl);
    assert!(held.is_pressed());
    assert_eq!(
        held.place(undo),
        undo.translated(1.0, 1.0),
        "押した見た目は右下へずらす"
    );
    assert_eq!(
        ctl.handle(PointerEvent::Up, b, &s),
        [ToolbarAction::Command("edit.undo")]
    );
    down(&mut ctl, b, &s, center(undo));
    mv(&mut ctl, b, &s, center(redo));
    assert!(
        !ButtonLook::of(&cell, &s, &ctl).is_pressed(),
        "外へずらせば押した見た目を外す"
    );
    assert!(
        ctl.handle(PointerEvent::Up, b, &s).is_empty(),
        "別のボタンの上で離しても実行しない"
    );
    down(&mut ctl, b, &s, center(undo));
    mv(&mut ctl, b, &s, center(undo));
    assert_eq!(
        ctl.handle(PointerEvent::Up, b, &s).len(),
        1,
        "戻せば実行する"
    );
    down(&mut ctl, b, &s, center(undo));
    ctl.handle(PointerEvent::Cancel, b, &s);
    assert!(ctl.handle(PointerEvent::Up, b, &s).is_empty());
}

#[test]
fn 押したあと押せなくなったボタンは離しても実行しない() {
    let b = bar(1000.0);
    let mut s = state();
    let split = toolbar_button(b, "clip.split").unwrap();
    let mut ctl = ToolbarController::default();
    down(&mut ctl, b, &s, center(split));
    s.disabled = vec!["clip.split"];
    assert!(ctl.handle(PointerEvent::Up, b, &s).is_empty());
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
    let slider = cells(b).slider.unwrap();
    let y = slider.rect.y + slider.rect.h / 2.0;
    let mut ctl = ToolbarController::default();
    let press = ctl.handle(
        PointerEvent::Down {
            x: slider.track.x,
            y,
            mods: Mods::default(),
        },
        b,
        &s,
    );
    assert_eq!(press, [ToolbarAction::Zoom(s.floor)]);
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
