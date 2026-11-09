use super::*;
use crate::controller::Mods;
use crate::palette::Palette;
use crate::screen::split_viewers;
use crate::text::tests::FakeMeasure;
use crate::text::UiFont;

fn layout(with_source: bool) -> SurfaceLayout {
    split_viewers(1000.0, 800.0, with_source)
}

fn state(duration: f64) -> TransportState {
    TransportState {
        program: ViewerState {
            duration,
            fps: 30.0,
            ..ViewerState::default()
        },
        source: None,
    }
}

fn with_source(mut s: TransportState) -> TransportState {
    s.source = Some(ViewerState {
        duration: 4.0,
        fps: 24.0,
        ..ViewerState::default()
    });
    s
}

fn down(x: f32, y: f32) -> PointerEvent {
    PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    }
}

/// ボタンの中央。
fn center(bar: Rect, viewer: Viewer, command: &str) -> (f32, f32) {
    let slot = cells(bar, viewer)
        .buttons
        .into_iter()
        .find(|b| b.spec.command == command)
        .unwrap_or_else(|| panic!("{command} のボタン"));
    (
        slot.rect.x + slot.rect.w / 2.0,
        slot.rect.y + slot.rect.h / 2.0,
    )
}

#[test]
fn バーは映像の真下に同じ幅で付き_映像の高さはバーの分だけ減る() {
    let l = layout(false);
    assert_eq!(l.program_bar.y, l.program.bottom());
    assert_eq!(
        (l.program_bar.x, l.program_bar.w),
        (l.program.x, l.program.w)
    );
    assert_eq!(l.program_bar.h, TRANSPORT_H);
    assert_eq!(l.source_bar, None);
    let l = layout(true);
    let (source, bar) = (l.source.expect("Source"), l.source_bar.expect("バー"));
    assert_eq!((bar.x, bar.y, bar.w), (source.x, source.bottom(), source.w));
    assert_eq!(l.program_bar.bottom() + 4.0, l.timeline.y);
}

#[test]
fn 低いサーフェスでもバーは上の段からはみ出さない() {
    let l = split_viewers(600.0, 200.0, true);
    assert!(l.program_bar.bottom() <= l.timeline.y);
    assert!(l.program.h >= 0.0);
}

#[test]
fn ボタンは並びの順に重ならず_バーの中央に寄り_時刻の欄に掛からない() {
    for (viewer, n) in [(Viewer::Program, 7), (Viewer::Source, 8)] {
        let bar = Rect::new(0.0, 100.0, 470.0, TRANSPORT_H);
        let c = cells(bar, viewer);
        assert_eq!(c.buttons.len(), n);
        for pair in c.buttons.windows(2) {
            assert!(pair[0].rect.right() <= pair[1].rect.x, "{viewer:?}");
        }
        let (first, last) = (c.buttons[0].rect, c.buttons[n - 1].rect);
        assert!(first.x >= c.time.right() && last.right() <= c.side.x);
        let gap = (first.x - bar.x) - (bar.right() - last.right());
        assert!(gap.abs() < 1e-3, "中央に寄る: {gap}");
        assert!(c.buttons.iter().all(|b| b.rect.y >= c.jog.bottom()));
    }
}

#[test]
fn 狭いバーではボタンの幅を縮めて時刻の欄に掛けない() {
    let bar = Rect::new(0.0, 0.0, 330.0, TRANSPORT_H);
    let c = cells(bar, Viewer::Source);
    let (first, last) = (c.buttons[0].rect, c.buttons[7].rect);
    assert!(first.w < BUTTON_W);
    assert!(first.x >= c.time.right() - 1e-3 && last.right() <= c.side.x + 1e-3);
}

#[test]
fn ボタンの当たり判定はボタンの矩形で_間と時刻の欄は何も無い() {
    let l = layout(false);
    let bar = l.program_bar;
    let (x, y) = center(bar, Viewer::Program, "playback.step_back");
    assert_eq!(
        hit(bar, Viewer::Program, x, y),
        Some(Target::Button("playback.step_back"))
    );
    let c = cells(bar, Viewer::Program);
    let between = c.buttons[0].rect.right() + 0.5;
    assert_eq!(hit(bar, Viewer::Program, between, y), None);
    assert_eq!(hit(bar, Viewer::Program, c.time.x + 4.0, y), None);
    assert_eq!(
        hit(bar, Viewer::Program, bar.x + 1.0, bar.y + 2.0),
        Some(Target::Jog)
    );
}

#[test]
fn ボタンを押すとそのコマンドの_id_を返す() {
    let l = layout(true);
    let s = with_source(state(10.0));
    let mut ctl = TransportController::default();
    let (x, y) = center(l.program_bar, Viewer::Program, "playback.toggle");
    assert_eq!(
        ctl.handle(down(x, y), &l, &s),
        [TransportAction::Command("playback.toggle")]
    );
    let source_bar = l.source_bar.expect("バー");
    for id in ["source.mark_in", "source.clear_in_out", "clip.overwrite"] {
        let (x, y) = center(source_bar, Viewer::Source, id);
        assert_eq!(
            ctl.handle(down(x, y), &l, &s),
            [TransportAction::Command(id)]
        );
    }
}

#[test]
fn 押せないボタンは何も返さない() {
    let l = layout(true);
    let mut s = with_source(state(10.0));
    s.source.as_mut().expect("Source").disabled = vec!["clip.insert"];
    let mut ctl = TransportController::default();
    let bar = l.source_bar.expect("バー");
    let (x, y) = center(bar, Viewer::Source, "clip.insert");
    assert!(ctl.handle(down(x, y), &l, &s).is_empty());
    let (x, y) = center(bar, Viewer::Source, "clip.overwrite");
    assert_eq!(ctl.handle(down(x, y), &l, &s).len(), 1);
}

#[test]
fn ジョグバーは押した位置とドラッグ中の位置へシークし_離すと止まる() {
    let l = layout(false);
    let s = state(10.0);
    let mut ctl = TransportController::default();
    let track = cells(l.program_bar, Viewer::Program).track();
    let y = l.program_bar.y + 4.0;
    let at = |frac: f32| track.x + track.w * frac;
    let seek = |t: f64| TransportAction::Seek {
        viewer: Viewer::Program,
        t,
    };
    assert_eq!(ctl.handle(down(at(0.5), y), &l, &s), [seek(5.0)]);
    // ドラッグは映像の上まで出ても追い、線の外は端に寄せる。
    let out = ctl.handle(
        PointerEvent::Move {
            x: at(1.5),
            y: 10.0,
        },
        &l,
        &s,
    );
    assert_eq!(out, [seek(10.0)]);
    let out = ctl.handle(
        PointerEvent::Move {
            x: at(-0.5),
            y: 10.0,
        },
        &l,
        &s,
    );
    assert_eq!(out, [seek(0.0)]);
    assert!(ctl.handle(PointerEvent::Up, &l, &s).is_empty());
    assert!(ctl
        .handle(PointerEvent::Move { x: at(0.2), y }, &l, &s)
        .is_empty());
}

#[test]
fn 長さの無いバーのジョグバーはシークしない() {
    let l = layout(false);
    let mut ctl = TransportController::default();
    let track = cells(l.program_bar, Viewer::Program).track();
    assert!(ctl
        .handle(down(track.x + 10.0, l.program_bar.y + 4.0), &l, &state(0.0))
        .is_empty());
}

#[test]
fn source_のジョグバーは_source_へシークする() {
    let l = layout(true);
    let s = with_source(state(10.0));
    let bar = l.source_bar.expect("バー");
    let track = cells(bar, Viewer::Source).track();
    let mut ctl = TransportController::default();
    let out = ctl.handle(down(track.x + track.w / 4.0, bar.y + 4.0), &l, &s);
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
    let (x, y) = center(l.program_bar, Viewer::Program, "playback.go_to_end");
    ctl.handle(PointerEvent::Move { x, y }, &l, &s);
    assert_eq!(
        ctl.hover(),
        Some((Viewer::Program, Target::Button("playback.go_to_end")))
    );
    ctl.handle(PointerEvent::Leave, &l, &s);
    assert_eq!(ctl.hover(), None);
}

fn texts(l: &SurfaceLayout, s: &TransportState) -> Vec<String> {
    transport_labels(
        l,
        s,
        &Palette::DEFAULT,
        &UiFont::default(),
        &mut FakeMeasure::default(),
    )
    .into_iter()
    .map(|l| l.text.to_string())
    .collect()
}

#[test]
fn program_のバーは再生位置と全体の長さをタイムコードで出す() {
    let mut s = state(65.5);
    s.program.t = 3.0 + 7.0 / 30.0;
    assert_eq!(texts(&layout(false), &s), ["00:00:03:07", "00:01:05:15"]);
}

#[test]
fn source_のバーは再生位置とイン点とアウト点を出し_無い点は破線にする() {
    let mut s = with_source(state(10.0));
    {
        let src = s.source.as_mut().expect("Source");
        src.t = 1.5;
        src.mark_in = Some(1.0);
    }
    assert_eq!(
        texts(&layout(true), &s)[2..],
        ["00:00:01:12", "In 00:00:01:00", "Out --:--:--:--"]
    );
}

#[test]
fn 押せないボタンは薄く_ポインターが乗ったボタンは下地が付く() {
    let l = layout(false);
    let mut s = state(10.0);
    s.program.disabled = vec!["playback.step_back"];
    let mut ctl = TransportController::default();
    let count = |ctl: &TransportController, s: &TransportState| {
        transport_quads(&l, s, ctl, &Palette::DEFAULT)
    };
    let plain = count(&ctl, &s);
    let p = Palette::DEFAULT;
    let dim = p.label.with_alpha(0.3);
    assert!(plain.iter().any(|q| q.color == dim), "薄い絵柄がある");
    assert!(plain.iter().any(|q| q.color == p.label), "押せる絵柄がある");
    let (x, y) = center(l.program_bar, Viewer::Program, "playback.toggle");
    ctl.handle(PointerEvent::Move { x, y }, &l, &s);
    let hovered = count(&ctl, &s);
    assert!(hovered.len() > plain.len(), "下地が足される");
    assert!(hovered.iter().any(|q| q.color == p.selection));
}

#[test]
fn 再生中の_play_stop_は停止の四角になり_止まっていれば三角() {
    let l = layout(false);
    let mut s = state(10.0);
    let shapes = |s: &TransportState| -> Vec<crate::geom::QuadShape> {
        let c = cells(l.program_bar, Viewer::Program);
        let slot = c
            .buttons
            .iter()
            .find(|b| b.spec.icon == Icon::PlayStop)
            .unwrap();
        transport_quads(&l, s, &TransportController::default(), &Palette::DEFAULT)
            .into_iter()
            .filter(|q| {
                slot.rect
                    .contains(q.rect.x + q.rect.w / 2.0, q.rect.y + q.rect.h / 2.0)
                    && q.color == Palette::DEFAULT.label
            })
            .map(|q| q.shape)
            .collect()
    };
    assert_eq!(shapes(&s), [crate::geom::QuadShape::TriRight]);
    s.program.playing = true;
    assert_eq!(shapes(&s), [crate::geom::QuadShape::Rect]);
}

#[test]
fn ジョグバーは進んだ分を塗り_source_はイン点とアウト点の間を塗る() {
    let l = layout(true);
    let mut s = with_source(state(10.0));
    s.program.t = 5.0;
    {
        let src = s.source.as_mut().expect("Source");
        src.mark_in = Some(1.0);
        src.mark_out = Some(3.0);
    }
    let p = Palette::DEFAULT;
    let quads = transport_quads(&l, &s, &TransportController::default(), &p);
    let fill = |bar: Rect, alpha: f32| {
        quads
            .iter()
            .find(|q| q.color == p.solo.with_alpha(alpha) && bar.contains(q.rect.x, q.rect.y))
            .map(|q| q.rect)
    };
    let pt = cells(l.program_bar, Viewer::Program).track();
    let fp = fill(l.program_bar, 0.7).expect("進んだ分");
    assert!((fp.w - pt.w / 2.0).abs() < 1e-3);
    let st = cells(l.source_bar.expect("バー"), Viewer::Source).track();
    let fs = fill(l.source_bar.expect("バー"), 0.5).expect("範囲");
    assert!((fs.x - (st.x + st.w / 4.0)).abs() < 1e-3);
    assert!((fs.w - st.w / 2.0).abs() < 1e-3);
}
