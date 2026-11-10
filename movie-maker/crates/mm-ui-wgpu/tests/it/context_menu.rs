//! 右クリックのメニューの点が、倍率が違ってもサーフェスの物理ピクセルで返ること、Timeline の外とツールバーの上では出ないこと。

use mm_ui_layout::{Action, Item, ItemKind, PointerEvent, Rect, TimelineModel};
use mm_ui_layout::{Place, TimelineLayout};
use mm_ui_wgpu::Ui;

use crate::support::{bar_rect, gpu, FORMAT, H, W};

fn ui_at(scale: f32) -> Option<(Ui, TimelineModel)> {
    let (device, _queue) = gpu()?;
    let model = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 4.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_scale_factor(scale);
    ui.set_model(model.clone());
    ui.resize(W, H);
    Some((ui, model))
}

fn menu_at(ui: &mut Ui, (x, y): (f32, f32)) -> Option<(Place, (f32, f32))> {
    ui.pointer(PointerEvent::ContextMenu { x, y })
        .into_iter()
        .find_map(|a| match a {
            Action::ContextMenu { hit, at, .. } => Place::of(&hit).map(|p| (p, at)),
            _ => None,
        })
}

#[test]
fn 右クリックの点は倍率が違ってもクリックした物理ピクセルで返る() {
    for scale in [1.0, 1.5] {
        let Some((mut ui, model)) = ui_at(scale) else {
            return;
        };
        let bar: Rect = bar_rect(&ui, &model, scale, 0, (0.0, 4.0));
        let click = (bar.x + bar.w / 2.0, bar.y + bar.h / 2.0);
        let (place, at) = menu_at(&mut ui, click).expect("帯の上で出る");
        assert_eq!(place, Place::Layer, "倍率 {scale}");
        assert!(
            (at.0 - click.0).abs() < 0.01 && (at.1 - click.1).abs() < 0.01,
            "倍率 {scale}: {at:?} と {click:?}"
        );
    }
}

#[test]
fn timeline_の外とツールバーの上の右クリックはメニューを出さない() {
    let Some((mut ui, _)) = ui_at(1.0) else {
        return;
    };
    let t = ui.layout().timeline;
    assert!(menu_at(&mut ui, (t.x / 2.0, t.y + 40.0)).is_none(), "外");
    let layout = TimelineLayout::new((t.w, t.h), 0.0);
    let bar = layout.toolbar;
    assert!(
        menu_at(&mut ui, (t.x + bar.x + 2.0, t.y + bar.y + 2.0)).is_none(),
        "ツールバー"
    );
}
