use super::*;
use crate::controller::Mods;
use crate::geom::Rect;
use crate::screen::split_viewers;
use crate::transport::jog_track;
use crate::transport::tests::{center, state, with_source};

fn layout(with_source: bool) -> SurfaceLayout {
    split_viewers(1000.0, 800.0, with_source)
}

fn down(x: f32, y: f32) -> PointerEvent {
    PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    }
}

fn moved((x, y): (f32, f32)) -> PointerEvent {
    PointerEvent::Move { x, y }
}

/// ボタンを押して、その場で離す。
fn click(
    ctl: &mut TransportController,
    l: &SurfaceLayout,
    s: &TransportState,
    (x, y): (f32, f32),
) -> Vec<TransportAction> {
    let mut out = ctl.handle(down(x, y), l, s);
    out.extend(ctl.handle(PointerEvent::Up, l, s));
    out
}

fn program_transport(l: &SurfaceLayout) -> Rect {
    l.program_transport
}

#[test]
fn ボタンは押した時には何もせず_離した時にそのコマンドの_id_を返す() {
    let l = layout(true);
    let s = with_source(state(10.0));
    let mut ctl = TransportController::default();
    let at = center(
        program_transport(&l),
        Viewer::Program,
        "playback.play_forward",
    );
    assert!(
        ctl.handle(down(at.0, at.1), &l, &s).is_empty(),
        "押しただけ"
    );
    assert_eq!(
        ctl.pressed(),
        Some((Viewer::Program, "playback.play_forward"))
    );
    assert_eq!(
        ctl.handle(PointerEvent::Up, &l, &s),
        [TransportAction::Command("playback.play_forward")]
    );
    assert_eq!(ctl.pressed(), None);
    let source_transport = l.source_transport.expect("Source の再生の操作のバー");
    for id in ["source.mark_in", "source.clear_in_out", "clip.overwrite"] {
        let at = center(source_transport, Viewer::Source, id);
        assert_eq!(click(&mut ctl, &l, &s, at), [TransportAction::Command(id)]);
    }
}

#[test]
fn 押したままボタンの外で離すと取りやめ_戻って離せば実行する() {
    let l = layout(false);
    let s = state(10.0);
    let mut ctl = TransportController::default();
    let at = center(program_transport(&l), Viewer::Program, "playback.go_to_end");
    let off = (at.0, l.program.y + 10.0);
    ctl.handle(down(at.0, at.1), &l, &s);
    ctl.handle(moved(off), &l, &s);
    assert_eq!(ctl.pressed(), None, "外では押した見た目にしない");
    assert!(
        ctl.handle(PointerEvent::Up, &l, &s).is_empty(),
        "外で離すと取りやめ"
    );

    ctl.handle(down(at.0, at.1), &l, &s);
    ctl.handle(moved(off), &l, &s);
    ctl.handle(moved(at), &l, &s);
    assert_eq!(
        ctl.handle(PointerEvent::Up, &l, &s),
        [TransportAction::Command("playback.go_to_end")]
    );
}

#[test]
fn 別のボタンの上で離しても_取り消しでも実行しない() {
    let l = layout(false);
    let s = state(10.0);
    let mut ctl = TransportController::default();
    let bar = program_transport(&l);
    let start = center(bar, Viewer::Program, "playback.go_to_start");
    let end = center(bar, Viewer::Program, "playback.go_to_end");
    ctl.handle(down(start.0, start.1), &l, &s);
    ctl.handle(moved(end), &l, &s);
    assert!(
        ctl.handle(PointerEvent::Up, &l, &s).is_empty(),
        "別のボタン"
    );
    ctl.handle(down(start.0, start.1), &l, &s);
    assert!(ctl.handle(PointerEvent::Cancel, &l, &s).is_empty());
    assert!(
        ctl.handle(PointerEvent::Up, &l, &s).is_empty(),
        "取り消した後の離し"
    );
    ctl.handle(down(start.0, start.1), &l, &s);
    ctl.handle(PointerEvent::Leave, &l, &s);
    assert!(
        ctl.handle(PointerEvent::Up, &l, &s).is_empty(),
        "サーフェスの外"
    );
}

#[test]
fn 押せないボタンは押しても離しても何も返さない() {
    let l = layout(true);
    let mut s = with_source(state(10.0));
    s.source.as_mut().expect("Source").disabled = vec!["clip.insert"];
    let mut ctl = TransportController::default();
    let bar = l.source_transport.expect("Source の再生の操作のバー");
    let at = center(bar, Viewer::Source, "clip.insert");
    assert!(click(&mut ctl, &l, &s, at).is_empty());
    let at = center(bar, Viewer::Source, "clip.overwrite");
    assert_eq!(click(&mut ctl, &l, &s, at).len(), 1);
    // 押している間に押せなくなったボタンは、離しても実行しない。
    let at = center(bar, Viewer::Source, "clip.overwrite");
    ctl.handle(down(at.0, at.1), &l, &s);
    s.source.as_mut().expect("Source").disabled = vec!["clip.overwrite"];
    assert!(ctl.handle(PointerEvent::Up, &l, &s).is_empty());
}

#[test]
fn ジョグバーは押した位置とドラッグ中の位置へシークし_離すと止まる() {
    let l = layout(false);
    let s = state(10.0);
    let mut ctl = TransportController::default();
    let track = jog_track(&l, Viewer::Program).expect("ジョグバーの溝");
    let y = track.y + track.h / 2.0;
    let at = |frac: f32| track.x + track.w * frac;
    let seek = |t: f64| TransportAction::Seek {
        viewer: Viewer::Program,
        t,
    };
    assert_eq!(ctl.handle(down(at(0.5), y), &l, &s), [seek(5.0)]);
    // ドラッグは映像の領域の上まで出ても追い、溝の外は端に寄せる。
    assert_eq!(ctl.handle(moved((at(1.5), 10.0)), &l, &s), [seek(10.0)]);
    assert_eq!(ctl.handle(moved((at(-0.5), 10.0)), &l, &s), [seek(0.0)]);
    assert!(ctl.handle(PointerEvent::Up, &l, &s).is_empty());
    assert!(ctl.handle(moved((at(0.2), y)), &l, &s).is_empty());
}

#[test]
fn 長さの無いバーのジョグバーはシークしない() {
    let l = layout(false);
    let mut ctl = TransportController::default();
    let track = jog_track(&l, Viewer::Program).expect("ジョグバーの溝");
    let out = ctl.handle(down(track.x + 10.0, track.y + 4.0), &l, &state(0.0));
    assert!(out.is_empty());
}

#[test]
fn source_のジョグバーは_source_へシークする() {
    let l = layout(true);
    let s = with_source(state(10.0));
    let track = jog_track(&l, Viewer::Source).expect("ジョグバーの溝");
    let mut ctl = TransportController::default();
    let out = ctl.handle(down(track.x + track.w / 4.0, track.y + 4.0), &l, &s);
    assert_eq!(
        out,
        [TransportAction::Seek {
            viewer: Viewer::Source,
            t: 1.0
        }]
    );
}

#[test]
fn ポインターの下のボタンを覚え_出ると忘れる() {
    let l = layout(false);
    let s = state(10.0);
    let mut ctl = TransportController::default();
    let at = center(program_transport(&l), Viewer::Program, "playback.go_to_end");
    ctl.handle(moved(at), &l, &s);
    assert_eq!(
        ctl.hover(),
        Some((Viewer::Program, TransportPart::Button("playback.go_to_end")))
    );
    ctl.handle(PointerEvent::Leave, &l, &s);
    assert_eq!(ctl.hover(), None);
}
