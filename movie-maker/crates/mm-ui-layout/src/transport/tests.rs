use super::*;
use crate::geom::{Quad, QuadShape};
use crate::palette::Palette;
use crate::screen::split_viewers;
use crate::text::tests::FakeMeasure;
use crate::text::UiFont;

fn layout(with_source: bool) -> SurfaceLayout {
    split_viewers(1000.0, 800.0, with_source)
}

pub(super) fn state(duration: f64) -> TransportState {
    TransportState {
        program: ViewerState {
            duration,
            fps: 30.0,
            ..ViewerState::default()
        },
        source: None,
    }
}

pub(super) fn with_source(mut s: TransportState) -> TransportState {
    s.source = Some(ViewerState {
        duration: 4.0,
        fps: 24.0,
        ..ViewerState::default()
    });
    s
}

/// ボタンの中央。
pub(super) fn center(bar: Rect, viewer: Viewer, command: &str) -> (f32, f32) {
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
fn バーは映像の領域の真下に同じ幅で付き_映像の領域の高さはバーの分だけ減る() {
    let l = layout(false);
    assert_eq!(l.program_transport.y, l.program.bottom());
    assert_eq!(
        (l.program_transport.x, l.program_transport.w),
        (l.program.x, l.program.w)
    );
    assert_eq!(l.program_transport.h, TRANSPORT_H);
    assert_eq!(l.source_transport, None);
    let l = layout(true);
    let (source, bar) = (
        l.source.expect("Source"),
        l.source_transport.expect("Source の再生の操作のバー"),
    );
    assert_eq!((bar.x, bar.y, bar.w), (source.x, source.bottom(), source.w));
    assert_eq!(l.program_transport.bottom() + 4.0, l.timeline.y);
}

#[test]
fn 上からタイムコードの行_ジョグバー_ボタンの行の順に重ならずに並ぶ() {
    let bar = Rect::new(0.0, 100.0, 470.0, TRANSPORT_H);
    let c = cells(bar, Viewer::Program);
    assert_eq!((c.position.y, c.range.y), (bar.y, bar.y));
    assert_eq!(c.jog.y, c.position.bottom());
    assert!(c.buttons.iter().all(|b| b.rect.y == c.jog.bottom()));
    assert!(c.buttons.iter().all(|b| b.rect.bottom() <= bar.bottom()));
    assert_eq!(
        c.position.right(),
        c.range.x,
        "タイムコードの行の左の欄と右の欄は接する"
    );
    assert_eq!(
        (c.jog.x, c.jog.w),
        (bar.x, bar.w),
        "ジョグバーは全幅で受ける"
    );
}

#[test]
fn ボタンは並びの順に重ならず_行の中央に寄る() {
    for (viewer, n) in [(Viewer::Program, 7), (Viewer::Source, 8)] {
        let bar = Rect::new(0.0, 100.0, 470.0, TRANSPORT_H);
        let c = cells(bar, viewer);
        assert_eq!(c.buttons.len(), n);
        for pair in c.buttons.windows(2) {
            assert!(pair[0].rect.right() <= pair[1].rect.x, "{viewer:?}");
        }
        let (first, last) = (c.buttons[0].rect, c.buttons[n - 1].rect);
        let gap = (first.x - bar.x) - (bar.right() - last.right());
        assert!(gap.abs() < 1e-3, "中央に寄る: {gap}");
    }
}

#[test]
fn program_のバーに同じ形のボタンは無く_再生と停止は別のボタン() {
    let icons: Vec<Icon> = PROGRAM_BUTTONS.iter().map(|b| b.icon).collect();
    for (i, icon) in icons.iter().enumerate() {
        assert!(!icons[i + 1..].contains(icon), "{icon:?} が 2 つある");
    }
    let ids: Vec<&str> = PROGRAM_BUTTONS.iter().map(|b| b.command).collect();
    assert!(ids.contains(&"playback.play_forward") && ids.contains(&"playback.stop"));
    assert!(!ids.contains(&"playback.toggle"));
}

#[test]
fn source_の編集のボタンは幅が足りれば名前で出し_足りなければアイコンにする() {
    let names = |w: f32| -> Vec<bool> {
        let bar = Rect::new(0.0, 0.0, w, TRANSPORT_H);
        cells(bar, Viewer::Source)
            .buttons
            .iter()
            .map(|b| b.named)
            .collect()
    };
    // 既定のウィンドウ（幅 1600 論理ピクセル）で Source を開いたときのバーの幅。
    let wide = names(398.0);
    assert_eq!(
        wide,
        [true, true, true, false, false, false, true, true],
        "In、Out、Clear、Insert、Overwrite は名前"
    );
    assert!(names(260.0).iter().all(|n| !n), "狭いと全部アイコン");
    let program = Rect::new(0.0, 0.0, 900.0, TRANSPORT_H);
    assert!(cells(program, Viewer::Program)
        .buttons
        .iter()
        .all(|b| !b.named));
}

#[test]
fn 狭いバーではボタンの幅を縮めて行に収める() {
    let bar = Rect::new(0.0, 0.0, 200.0, TRANSPORT_H);
    let c = cells(bar, Viewer::Source);
    let (first, last) = (c.buttons[0].rect, c.buttons[7].rect);
    assert!(first.w < BUTTON_W);
    assert!(first.x >= bar.x + PAD - 1e-3 && last.right() <= bar.right() - PAD + 1e-3);
}

#[test]
fn ボタンの当たり判定はボタンの矩形で_間とタイムコードの行は何も無い() {
    let l = layout(false);
    let bar = l.program_transport;
    let (x, y) = center(bar, Viewer::Program, "playback.step_back");
    assert_eq!(
        part_in(bar, Viewer::Program, x, y),
        Some(TransportPart::Button("playback.step_back"))
    );
    let c = cells(bar, Viewer::Program);
    let between = c.buttons[0].rect.right() + 0.5;
    assert_eq!(part_in(bar, Viewer::Program, between, y), None);
    assert_eq!(
        part_in(bar, Viewer::Program, c.position.x + 4.0, c.position.y + 4.0),
        None
    );
    assert_eq!(
        part_in(bar, Viewer::Program, bar.x + 1.0, c.jog.y + 2.0),
        Some(TransportPart::Jog)
    );
}

#[test]
fn play_test_が押す位置はバーのボタンとジョグバーの溝() {
    let l = layout(true);
    let rect = transport_button(&l, Viewer::Source, "source.clear_in_out").expect("ボタン");
    let (x, y) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    assert_eq!(
        part_at(&l, x, y),
        Some((Viewer::Source, TransportPart::Button("source.clear_in_out")))
    );
    assert_eq!(
        transport_button(&l, Viewer::Program, "source.mark_in"),
        None
    );
    let track = jog_track(&l, Viewer::Program).expect("ジョグバーの溝");
    assert_eq!(
        part_at(&l, track.x, track.y + 1.0),
        Some((Viewer::Program, TransportPart::Jog))
    );
}

#[test]
fn 低いサーフェスではバーを出さずに映像に回す() {
    let l = split_viewers(600.0, 200.0, true);
    assert_eq!(l.program_transport.h, 0.0);
    assert_eq!(
        l.program.bottom() + 4.0,
        l.timeline.y,
        "映像の領域がビューアーの領域を全部使う"
    );
    assert!(transport_rect(&l, Viewer::Program).is_none());
    assert!(transport_quads(
        &l,
        &with_source(state(10.0)),
        &TransportController::default(),
        &Palette::DEFAULT
    )
    .is_empty());
    let l = split_viewers(600.0, 300.0, true);
    assert_eq!(
        l.program_transport.h, TRANSPORT_H,
        "映像の領域がバーの高さ以上残るなら出す"
    );
    assert!(l.program.h >= TRANSPORT_H);
}

fn texts(l: &SurfaceLayout, s: &TransportState) -> Vec<String> {
    transport_labels(
        l,
        s,
        &TransportController::default(),
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
fn source_のバーは再生位置とイン点とアウト点とボタンの名前を出し_無い点はハイフンにする() {
    let mut s = with_source(state(10.0));
    {
        let src = s.source.as_mut().expect("Source");
        src.t = 1.5;
        src.mark_in = Some(1.0);
    }
    assert_eq!(
        texts(&layout(true), &s)[2..],
        [
            "00:00:01:12",
            "In 00:00:01:00  Out --:--:--:--",
            "In",
            "Out",
            "Clear",
            "Insert",
            "Overwrite"
        ]
    );
}

/// ボタン `command` の矩形の中に中心がある矩形。
fn quads_in(quads: &[Quad], l: &SurfaceLayout, viewer: Viewer, command: &str) -> Vec<Quad> {
    let r = transport_button(l, viewer, command).expect("ボタン");
    quads
        .iter()
        .filter(|q| r.contains(q.rect.x + q.rect.w / 2.0, q.rect.y + q.rect.h / 2.0))
        .copied()
        .collect()
}

#[test]
fn 押せないボタンは薄く_ポインターが乗ったボタンは背景が付く() {
    let l = layout(false);
    let mut s = state(10.0);
    s.program.disabled = vec!["playback.step_back"];
    let mut ctl = TransportController::default();
    let p = Palette::DEFAULT;
    let plain = transport_quads(&l, &s, &ctl, &p);
    let dim = p.label.with_alpha(0.3);
    let back = quads_in(&plain, &l, Viewer::Program, "playback.step_back");
    assert!(back.iter().all(|q| q.color == dim), "薄いアイコン");
    let (x, y) = center(l.program_transport, Viewer::Program, "playback.stop");
    ctl.handle(crate::controller::PointerEvent::Move { x, y }, &l, &s);
    let hovered = quads_in(
        &transport_quads(&l, &s, &ctl, &p),
        &l,
        Viewer::Program,
        "playback.stop",
    );
    assert!(
        hovered
            .iter()
            .any(|q| q.color == p.selection.with_alpha(0.12)),
        "背景"
    );
    assert!(
        hovered.iter().any(|q| q.color == p.selection),
        "明るいアイコン"
    );
}

#[test]
fn 再生中は_play_forward_を入の色で塗る() {
    let l = layout(false);
    let mut s = state(10.0);
    let p = Palette::DEFAULT;
    let ctl = TransportController::default();
    let forward = |s: &TransportState| {
        quads_in(
            &transport_quads(&l, s, &ctl, &p),
            &l,
            Viewer::Program,
            "playback.play_forward",
        )
    };
    assert!(forward(&s)
        .iter()
        .all(|q| q.color == p.label && q.shape == QuadShape::TriRight));
    s.program.playing = true;
    let lit = forward(&s);
    assert!(
        lit.iter()
            .any(|q| q.color == p.accent && q.shape == QuadShape::Rect),
        "背景"
    );
    assert!(lit
        .iter()
        .any(|q| q.color == text_on_accent() && q.shape == QuadShape::TriRight));
}

fn text_on_accent() -> crate::geom::Color {
    crate::palette::text_on(Palette::DEFAULT.accent)
}

#[test]
fn ジョグバーは再生位置までを塗り_source_はイン点とアウト点の間を塗る() {
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
            .find(|q| q.color == p.accent.with_alpha(alpha) && bar.contains(q.rect.x, q.rect.y))
            .map(|q| q.rect)
    };
    let pt = jog_track(&l, Viewer::Program).expect("ジョグバーの溝");
    let fp = fill(l.program_transport, 0.7).expect("再生位置まで");
    assert!((fp.w - pt.w / 2.0).abs() < 1e-3);
    let st = jog_track(&l, Viewer::Source).expect("ジョグバーの溝");
    let fs = fill(l.source_transport.expect("Source の再生の操作のバー"), 0.5).expect("範囲");
    assert!((fs.x - (st.x + st.w / 4.0)).abs() < 1e-3);
    assert!((fs.w - st.w / 2.0).abs() < 1e-3);
}
