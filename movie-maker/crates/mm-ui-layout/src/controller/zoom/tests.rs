//! キーとメニューのコマンド（拡大縮小、全体表示、前後の帯の端）、ズームバーのドラッグ、再生中のページ送りの試験。

use super::super::tests::{ctl, down, move_to};
use super::*;
use crate::controller::{edit_target, Action, PointerEvent, TimelineCommand};
use crate::time_view::MIN_PX_PER_SEC;
use crate::timeline::{CutBar, Item, ItemKind, TimelineModel};
use crate::timeline_layout::HEADER_W;

/// 長さ `duration` 秒の、画像のレイヤー 2 本。
fn model(duration: f64) -> TimelineModel {
    TimelineModel {
        duration,
        fps: 30.0,
        items: vec![
            Item::layer(0.0, duration, ItemKind::Image, true),
            Item::layer(0.0, duration, ItemKind::Image, true),
        ],
        ..TimelineModel::default()
    }
}

/// 先頭がシーケンスのレイヤー（カット 0-3、3-4、4-7 秒）、次が 10 秒から 14 秒のレイヤー（12 秒にキーフレーム）。
/// 16 秒にマーカー。20 秒、30 fps。
fn edits() -> TimelineModel {
    let cut = |start, end| CutBar {
        start,
        end,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 7.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 3.0), cut(3.0, 4.0), cut(4.0, 7.0)];
    let mut text = Item::layer(10.0, 14.0, ItemKind::Text, true);
    text.keyframes = [12.0].into();
    TimelineModel {
        duration: 20.0,
        fps: 30.0,
        items: vec![seq, text],
        markers: vec![16.0],
    }
}

/// 行の領域とズームバーの幅が 2000 の Timeline の操作。1 秒 100 px で、0 秒から 20 秒が見えている。
/// 試験の x は [`ctl`] と同じく行の領域（ズームバーも同じ）の左端を 0 とする。
fn wide(m: &TimelineModel) -> TimelineController {
    let mut c = ctl(m);
    c.resize((2000.0 + HEADER_W, 300.0));
    c
}

/// ズームバーの縦の中央。
fn bar_y(c: &TimelineController) -> f32 {
    let z = c.layout().zoombar;
    z.y + z.h / 2.0
}

#[test]
fn キーの拡大縮小は見えている再生位置を動かさず_縮小は全体が収まる所で止まる() {
    let m = model(10.0);
    let mut c = ctl(&m);
    let x = c.view.x_of(5.0);
    c.command(TimelineCommand::ZoomIn, &m, 5.0);
    assert!((c.view.px_per_sec - 100.0 * ZOOM_KEY_STEP).abs() < 1e-9);
    assert!((c.view.x_of(5.0) - x).abs() < 1e-3, "再生位置の x は同じ");
    c.command(TimelineCommand::ZoomIn, &m, 5.0);
    assert!((c.view.px_per_sec - 200.0).abs() < 1e-9, "2 回で 2 倍");
    for _ in 0..40 {
        c.command(TimelineCommand::ZoomOut, &m, 5.0);
    }
    assert_eq!(
        c.view.px_per_sec, MIN_PX_PER_SEC,
        "短いコンポジションは下限まで"
    );
    assert!((c.view.x_of(5.0) - x).abs() < 1e-3);
}

#[test]
fn 再生位置が見えていなければ_拡大した後に再生位置を中央に置く() {
    let m = model(100.0);
    let mut c = ctl(&m);
    // 1 秒 100 px で 0 から 20 秒が見えている。
    c.command(TimelineCommand::ZoomIn, &m, 50.0);
    let middle = c.layout().row_area.w / 2.0;
    assert!((c.view.x_of(50.0) - middle).abs() < 1e-3);
    assert!((c.view.px_per_sec - 100.0 * ZOOM_KEY_STEP).abs() < 1e-9);
}

#[test]
fn 全体表示のコマンドは元の表示と行き来し_ほかの操作で変えた後はもう一度全体表示にする() {
    let m = model(100.0);
    let mut c = ctl(&m);
    let start = c.view;
    let fit = TimeView::fit(100.0, c.layout().row_area.w, VIEW_MARGIN);
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    assert_eq!(c.view, fit);
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    assert_eq!(c.view, start, "もう一度で元の倍率と位置");
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    c.command(TimelineCommand::ZoomIn, &m, 50.0);
    let zoomed = c.view;
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    assert_eq!(c.view, fit, "全体表示の後に拡大したら、もう一度全体表示");
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    assert_eq!(c.view, zoomed, "戻るのは拡大した表示");
}

#[test]
fn 全体表示は大きさを変えても全体表示のままとみなし_前の表示に戻せる() {
    let m = model(100.0);
    let mut c = ctl(&m);
    let start = c.view;
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    c.resize((1500.0, 300.0));
    c.command(TimelineCommand::FullExtentZoom, &m, 0.0);
    assert_eq!(c.view, start);
}

#[test]
fn ズームバーのスクロールボックスを引くと倍率を保って動き_離しても編集にならない() {
    let m = model(100.0);
    let mut c = wide(&m);
    // 100 秒を幅 2000 の溝に並べるので 1 px は 0.05 秒。スクロールボックスは x 0 から 400。
    let y = bar_y(&c);
    assert!(c.handle(down(200.0, y), &m, 0.0).is_empty());
    c.handle(move_to(300.0, y), &m, 0.0);
    assert!((c.view.origin - 5.0).abs() < 1e-9, "100 px で 5 秒");
    assert_eq!(c.view.px_per_sec, 100.0);
    assert_eq!(
        c.hover,
        Hit::ZoomBar {
            part: ZoomBarPart::Thumb
        },
        "掴んでいる間はスクロールボックスを明るく"
    );
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
}

#[test]
fn ズームバーの右端を引くと始まりを保って拡大縮小し_左端は終わりを保つ() {
    let m = model(100.0);
    let mut c = wide(&m);
    let y = bar_y(&c);
    c.handle(down(397.0, y), &m, 0.0);
    c.handle(move_to(797.0, y), &m, 0.0);
    c.handle(PointerEvent::Up, &m, 0.0);
    assert_eq!(c.view.origin, 0.0);
    assert!(
        (c.view.px_per_sec - 50.0).abs() < 1e-9,
        "終わりを 20 秒から 40 秒へ"
    );
    let thumb = zoombar::thumb_of(&c.viewport(), m.duration);
    let left = thumb.x - c.layout().zoombar.x;
    c.handle(down(left + 2.0, y), &m, 0.0);
    c.handle(move_to(left + 202.0, y), &m, 0.0);
    let (start, end) = c.view.visible(c.layout().row_area.w);
    assert!((start - 10.0).abs() < 1e-9 && (end - 40.0).abs() < 1e-9);
}

#[test]
fn ズームバーの溝を押すと押した側へ_1_画面ぶん送る() {
    let m = model(100.0);
    let mut c = wide(&m);
    let y = bar_y(&c);
    c.handle(down(1000.0, y), &m, 0.0);
    assert_eq!(c.view.origin, 20.0);
    c.handle(PointerEvent::Up, &m, 0.0);
    c.handle(down(10.0, y), &m, 0.0);
    assert_eq!(c.view.origin, 0.0);
}

#[test]
fn 再生位置が右端を越えたら_1_画面ぶん送り_右端の外から進んでも送らない() {
    let m = model(100.0);
    let mut c = wide(&m);
    c.page_scroll(19.9, 20.1);
    assert_eq!(c.view.origin, 20.0, "右端が左端になる");
    c.page_scroll(25.0, 26.0);
    assert_eq!(c.view.origin, 20.0, "見えている間は送らない");
    c.page_scroll(39.0, 75.0);
    assert_eq!(c.view.origin, 60.0, "越えた先が見えるまで");
    c.view.origin = 0.0;
    c.page_scroll(30.0, 31.0);
    assert_eq!(c.view.origin, 0.0, "人が動かした表示を保つ");
}

#[test]
fn 再生位置が左端を越えたら左へ送り_0_より前は見せない() {
    let m = model(100.0);
    let mut c = wide(&m);
    c.view.origin = 60.0;
    c.page_scroll(60.5, 59.0);
    assert_eq!(c.view.origin, 40.0, "左端が右端になる");
    c.page_scroll(41.0, 3.0);
    assert_eq!(c.view.origin, 0.0, "越えた先が見えるまで");
    c.view.origin = 5.0;
    c.page_scroll(10.0, 0.0);
    assert_eq!(
        c.view.origin, 0.0,
        "1 画面ぶん戻すと 0 より前になるときは 0 から"
    );
    c.view.origin = 50.0;
    c.page_scroll(10.0, 9.0);
    assert_eq!(c.view.origin, 50.0, "左端の外から動いても送らない");
}

#[test]
fn ドラッグの間は送らず_離した後の移動から送る() {
    let m = model(100.0);
    let mut c = wide(&m);
    let y = bar_y(&c);
    c.handle(down(200.0, y), &m, 0.0);
    c.page_scroll(19.9, 20.1);
    assert_eq!(c.view.origin, 0.0, "スクロールボックスを掴んでいる間");
    c.handle(PointerEvent::Up, &m, 0.0);
    c.page_scroll(19.9, 20.1);
    assert_eq!(c.view.origin, 20.0);
}

#[test]
fn 前後の帯の端は_0_とカットとレイヤーの端と終わりで_今いる端は数えない() {
    let m = edits();
    let next = |t| edit_target(&m, t, true);
    let previous = |t| edit_target(&m, t, false);
    assert_eq!(next(0.0), Some(3.0));
    assert_eq!(next(3.0), Some(4.0));
    assert_eq!(next(2.99), Some(4.0), "半フレームより近い端は今いる所");
    assert_eq!(next(7.0), Some(10.0));
    assert_eq!(next(10.0), Some(14.0), "キーフレームでは止まらない");
    assert_eq!(
        next(14.0),
        Some(20.0 - 1.0 / 30.0),
        "マーカーでは止まらず、終わりは最後のフレーム"
    );
    assert_eq!(next(20.0 - 1.0 / 30.0), None);
    assert_eq!(previous(3.5), Some(3.0));
    assert_eq!(previous(3.0), Some(0.0));
    assert_eq!(previous(12.0), Some(10.0));
    assert_eq!(previous(0.0), None);
}

#[test]
fn 前後の帯の端のコマンドは再生位置のシークを返す() {
    let m = edits();
    let mut c = ctl(&m);
    let start = c.view;
    assert_eq!(
        c.command(TimelineCommand::NextEdit, &m, 3.5),
        vec![Action::Seek(4.0)]
    );
    assert_eq!(
        c.command(TimelineCommand::PreviousEdit, &m, 3.5),
        vec![Action::Seek(3.0)]
    );
    assert!(c.command(TimelineCommand::PreviousEdit, &m, 0.0).is_empty());
    assert_eq!(c.view, start, "表示は変えない");
    assert!(c.command(TimelineCommand::ZoomIn, &m, 0.0).is_empty());
}
