use super::*;
use mm_ui_layout::timeline_height;

const GEO: Geo = Geo {
    w: 1000.0,
    h: 800.0,
    window_w: 1660.0,
};

fn drag_of(which: Splitter, ws: &Workspace, at: (f32, f32)) -> Drag {
    Drag {
        which,
        grab: grab_offset(ws, GEO, which, at),
        before: *ws,
    }
}

#[test]
fn 縦の仕切りを左へ動かすと_webview_の幅がその分広がる() {
    let ws = Workspace::default();
    // 仕切りの帯（x = 996 から 1000）の真ん中を掴む。
    let drag = drag_of(Splitter::Vertical, &ws, (998.0, 300.0));
    let next = dragged(&drag, &ws, GEO, (898.0, 300.0));
    assert_eq!(next.columns(GEO.window_w).web(), 760.0);
    assert_eq!(next.columns(GEO.window_w).right, 400.0);
}

#[test]
fn 動かさなければ割り付けは変わらない() {
    let ws = Workspace::default();
    for (which, at) in [
        (Splitter::Vertical, (998.0, 300.0)),
        (Splitter::Horizontal, (400.0, 438.0)),
    ] {
        let drag = drag_of(which, &ws, at);
        assert_eq!(dragged(&drag, &ws, GEO, at), ws, "{which:?}");
    }
}

#[test]
fn 横の仕切りを上へ動かすと_timeline_が高くなる() {
    let ws = Workspace::default();
    let drag = drag_of(Splitter::Horizontal, &ws, (400.0, 438.0));
    let next = dragged(&drag, &ws, GEO, (400.0, 338.0));
    assert_eq!(timeline_height(GEO.h, next.timeline_ratio), 460.0);
}

#[test]
fn 横の仕切りを下げすぎても_timeline_の下限の高さで止まる() {
    let ws = Workspace::default();
    let drag = drag_of(Splitter::Horizontal, &ws, (400.0, 438.0));
    let next = dragged(&drag, &ws, GEO, (400.0, 5000.0));
    // 比 0 で求めた高さが下限（目盛りとレイヤー 2 本）。
    assert_eq!(
        timeline_height(GEO.h, next.timeline_ratio),
        timeline_height(GEO.h, 0.0)
    );
}

#[test]
fn 縦の仕切りを右へ動かしすぎても_左の列が下限で止まり右の列を縮める() {
    let ws = Workspace::default();
    let drag = drag_of(Splitter::Vertical, &ws, (998.0, 300.0));
    let next = dragged(&drag, &ws, GEO, (1600.0, 300.0));
    let c = next.columns(GEO.window_w);
    assert_eq!((c.left, c.right), (200.0, 300.0));
}

#[test]
fn 列が全部閉じていれば縦の仕切りは無く_横の仕切りが右端まで届く() {
    let ws = Workspace {
        left_open: false,
        right_open: false,
        ..Workspace::default()
    };
    assert_eq!(hit(&ws, GEO, (999.0, 100.0)), None);
    let gap_y = 800.0 - timeline_height(800.0, ws.timeline_ratio) - 2.0;
    assert_eq!(hit(&ws, GEO, (999.0, gap_y)), Some(Splitter::Horizontal));
}

#[test]
fn 状態の符号は仕切りと対応する() {
    for which in [None, Some(Splitter::Vertical), Some(Splitter::Horizontal)] {
        let code = hot_code(which);
        let back = match code {
            1 => Some(Splitter::Vertical),
            2 => Some(Splitter::Horizontal),
            _ => None,
        };
        assert_eq!(back, which);
    }
}
