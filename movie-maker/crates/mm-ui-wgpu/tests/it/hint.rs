//! ポインターの下の手がかり。ステータスバーの文、ツールチップの中身、カーソルを決める値（`Pointee`）を、実際の `Ui` で確かめる。

use mm_ui_layout::{
    cursor_for, toolbar_button, CursorKind, CutBar, Item, ItemKind, Mods, PointerEvent,
    TimelineModel, Tool, TooltipContent, Under,
};
use mm_ui_wgpu::Ui;

use crate::support::{bar_rect, gpu, render_sized, FORMAT, H, W};

fn model() -> TimelineModel {
    let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true);
    seq.cuts = vec![
        CutBar {
            start: 0.0,
            end: 3.0,
            label: "a.mp4".into(),
            ..CutBar::default()
        },
        CutBar {
            start: 3.0,
            end: 6.0,
            label: "b.mp4".into(),
            ..CutBar::default()
        },
    ];
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![seq],
        ..TimelineModel::default()
    }
}

fn open() -> Option<(wgpu::Device, wgpu::Queue, Ui, TimelineModel)> {
    let (device, queue) = gpu()?;
    let model = model();
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model.clone());
    ui.resize(W, H);
    // 最初の描画でツールバーの名前の幅が決まる（ツールバーのボタンの位置を引くのに要る）。
    render_sized(&device, &queue, &mut ui, (W, H));
    Some((device, queue, ui, model))
}

#[test]
fn カットの端にポインターを乗せると_ステータスバーに_ripple_と_roll_の操作が出る() {
    let Some((device, queue, mut ui, model)) = open() else {
        return;
    };
    let bar = bar_rect(&ui, &model, 1.0, 0, (3.0, 3.0));
    let (x, y) = (bar.x - 2.0, bar.y + bar.h / 2.0);
    ui.pointer(PointerEvent::Move { x, y });
    render_sized(&device, &queue, &mut ui, (W, H));
    let (left, right) = ui.status_text();
    assert_eq!(left, "Drag: Ripple Trim \u{B7} Alt+Drag: Roll");
    assert_eq!(right, "Snapping On \u{B7} Selection Tool");
    let alt = Mods {
        shift: false,
        alt: true,
    };
    let p = ui.pointee();
    assert_eq!(cursor_for(&p, Mods::default()), CursorKind::SizeWE);
    assert_eq!(cursor_for(&p, alt), CursorKind::Roll);
    assert_eq!(ui.hint().at, Some((x, y)));
}

#[test]
fn ドラッグ中は動かした量と取り消しのキーを出し_離すと元に戻る() {
    let Some((device, queue, mut ui, model)) = open() else {
        return;
    };
    let bar = bar_rect(&ui, &model, 1.0, 0, (3.0, 3.0));
    let (x, y) = (bar.x - 2.0, bar.y + bar.h / 2.0);
    let mods = Mods::default();
    ui.pointer(PointerEvent::Move { x, y });
    ui.pointer(PointerEvent::Down { x, y, mods });
    ui.pointer(PointerEvent::Move { x: x - 30.0, y });
    render_sized(&device, &queue, &mut ui, (W, H));
    let left = &ui.status_text().0;
    assert!(
        left.starts_with("Ripple Trim \u{2212}")
            && left.ends_with("N: Toggle Snapping \u{B7} Esc: Cancel"),
        "{left}"
    );
    assert_eq!(
        ui.hint().tooltip,
        None,
        "ドラッグ中はツールチップを出さない"
    );
    ui.pointer(PointerEvent::Cancel);
    render_sized(&device, &queue, &mut ui, (W, H));
    assert!(ui.status_text().0.starts_with("Drag: Ripple Trim"));
}

#[test]
fn ツールバーのボタンの上では_ツールチップはコマンドの_id_でステータスバーは名前() {
    let Some((device, queue, mut ui, _)) = open() else {
        return;
    };
    let bar = ui.layout().timeline;
    let local = toolbar_button(
        mm_ui_layout::Rect::new(0.0, 0.0, bar.w, 30.0),
        ui.toolbar_names(),
        "timeline.blade_tool",
    )
    .expect("Blade のボタン");
    let at = (
        bar.x + local.x + local.w / 2.0,
        bar.y + local.y + local.h / 2.0,
    );
    let ev = PointerEvent::Move { x: at.0, y: at.1 };
    ui.toolbar_pointer(ev);
    ui.pointer(ev);
    render_sized(&device, &queue, &mut ui, (W, H));
    let hint = ui.hint();
    assert_eq!(
        hint.pointee.under,
        Under::Toolbar(mm_ui_layout::ToolbarPart::Button("timeline.blade_tool"))
    );
    assert_eq!(
        hint.tooltip,
        Some(TooltipContent::Command("timeline.blade_tool"))
    );
    assert_eq!(ui.status_text().0, "Click: Blade Tool");
    ui.pointer(PointerEvent::Leave);
    assert_eq!(ui.hint().tooltip, None);
}

#[test]
fn 帯の上のツールチップは種類と名前と範囲と長さ() {
    let Some((_, _, mut ui, model)) = open() else {
        return;
    };
    let bar = bar_rect(&ui, &model, 1.0, 0, (0.0, 3.0));
    ui.pointer(PointerEvent::Move {
        x: bar.x + bar.w / 2.0,
        y: bar.y + bar.h / 2.0,
    });
    let Some(TooltipContent::Text(text, _)) = ui.hint().tooltip else {
        panic!("帯のツールチップ");
    };
    assert_eq!(
        text,
        "Clip \u{B7} a.mp4 \u{B7} 00:00:00:00\u{2013}00:00:03:00 (3:00)"
    );
    assert_eq!(ui.pointee().tool, Tool::Selection);
}
