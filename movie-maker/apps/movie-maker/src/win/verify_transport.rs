//! `--play-test` の再生の操作のバーの通し。実際のウィンドウへのクリックで、Program のバーのボタン（末尾、先頭、1 フレーム送り、
//! 再生と停止）、押したままボタンの外で離す取りやめ、ジョグバーのクリックとドラッグ、Source のバーのジョグバーとイン点・アウト点と
//! Clear のボタンを押し、再生位置とイン点・アウト点が期待どおり変わったかを確かめる。ウィンドウが前面なら、Alt+X
//! （Clear In and Out）を `SendInput` で押す（投げたメッセージではキーの状態が変わらず、Alt の付いたキーにならないため）。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use mm_ui_layout::Viewer;
use mm_ui_wgpu::timeline_model;
use serde_json::{json, Map, Value};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::playback::PlaybackInfo;
use crate::win::source::{marks_of, SourceHandle};
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify::scrub_tolerance;
use crate::win::verify_place::{placement, Placement};
use crate::win::verify_stage::{checkpoint, wait_until};

/// X キーの仮想キーコード。
const VK_X: VIRTUAL_KEY = VIRTUAL_KEY(0x58);
/// 入力を投げてから結果を見るまでの待ち。
const SETTLE: Duration = Duration::from_millis(300);
/// 結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// Program のジョグバーでクリックする位置と、ドラッグの始めと終わり（全体の長さに対する比）。
const JOG_CLICK: f32 = 0.5;
const JOG_FROM: f32 = 0.25;
const JOG_TO: f32 = 0.75;
/// Source のジョグバーでイン点とアウト点を付ける位置（素材の長さに対する比）。
const MARK_IN_AT: f32 = 0.25;
const MARK_OUT_AT: f32 = 0.75;

/// 試験の対象。
pub struct Env<'a> {
    pub shared: &'a Arc<Shared>,
    pub info: &'a PlaybackInfo,
    pub surface_hwnd: isize,
    pub window_hwnd: isize,
    pub test: &'a PlayTest,
}

/// 結果の記録。手順ごとの合否と、撮ったスクリーンショット。
#[derive(Default)]
struct Log {
    steps: Map<String, Value>,
    shots: Vec<String>,
}

impl Log {
    fn step(&mut self, name: &str, ok: bool, detail: Value) -> bool {
        self.steps
            .insert(name.to_owned(), json!({ "ok": ok, "detail": detail }));
        ok
    }

    /// 確かめられなかった手順。合否には数えない。
    fn skip(&mut self, name: &str, reason: &str) {
        self.steps
            .insert(name.to_owned(), json!({ "skipped": reason }));
    }

    fn into_json(self) -> Value {
        let all_ok = self.steps.values().all(|v| v["ok"] != false);
        json!({ "all_ok": all_ok, "steps": self.steps, "screenshots": self.shots })
    }
}

/// 今の割り付け。
fn current_place(shared: &Shared, hwnd: isize) -> Option<Placement> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    placement(shared, hwnd, &timeline_model(&doc.project, comp))
}

/// バーのボタンを押して離す。バーを出していないか、ボタンが無ければ false。
fn press(env: &Env<'_>, place: &Placement, viewer: Viewer, id: &str) -> bool {
    let Some((x, y)) = place.transport_button(viewer, id) else {
        return false;
    };
    surface::post_click(env.surface_hwnd, x, y);
    true
}

/// ジョグバーの溝の、比 `frac` の位置（クライアントの物理ピクセル）。
fn jog_at(place: &Placement, viewer: Viewer, frac: f32) -> Option<(i32, i32)> {
    let t = place.jog_track(viewer)?;
    Some(place.surface_point((t.x + t.w * frac, t.y + t.h / 2.0)))
}

/// ジョグバーのクリックでシークした結果の許容誤差（秒）。Timeline のスクラブと同じく、半画素と半フレームの和。
fn jog_tolerance(place: &Placement, viewer: Viewer, duration: f64, fps: f64) -> f64 {
    let w = place.jog_track(viewer).map_or(1.0, |t| t.w);
    scrub_tolerance(f64::from(w * place.scale.get()) / duration.max(1e-9), fps)
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
fn shot(env: &Env<'_>, log: &mut Log, name: &str) {
    let Some(base) = &env.test.screenshot else {
        return;
    };
    std::thread::sleep(SETTLE);
    let path = base.with_extension(format!("transport-{name}.png"));
    match capture::capture_window(env.window_hwnd, &path) {
        Ok(()) => log.shots.push(path.display().to_string()),
        Err(e) => log.shots.push(format!("失敗: {e}")),
    }
    checkpoint();
}

/// Program のバーの末尾、先頭、1 フレーム送り、再生と停止のボタン。
fn program_steps(env: &Env<'_>, log: &mut Log, place: &Placement) {
    let shared = env.shared;
    let fps = env.info.fps;
    let near = |t: f64| move || (shared.playhead() - t).abs() <= 0.5 / fps;
    let last = (env.info.duration - 1.0 / fps).max(0.0);
    for (id, t) in [
        ("playback.go_to_end", last),
        ("playback.go_to_start", 0.0),
        ("playback.step_forward", 1.0 / fps),
    ] {
        let ok = press(env, place, Viewer::Program, id) && wait_until(WAIT, near(t));
        log.step(
            id,
            ok,
            json!({ "expected": t, "playhead": shared.playhead() }),
        );
    }
    let playing = || shared.playing.load(Ordering::SeqCst);
    let started =
        press(env, place, Viewer::Program, "playback.play_forward") && wait_until(WAIT, playing);
    shot(env, log, "playing");
    let stopped =
        press(env, place, Viewer::Program, "playback.stop") && wait_until(WAIT, || !playing());
    log.step(
        "play_forward_and_stop",
        started && stopped,
        json!({ "started": started, "stopped": stopped }),
    );
}

/// Go to End を押したまま映像の領域の上へずらして離すと、何も起きない。
fn release_off_step(env: &Env<'_>, log: &mut Log, place: &Placement) {
    let Some(from) = place.transport_button(Viewer::Program, "playback.go_to_end") else {
        log.step("release_off_button", false, json!("ボタンが無い"));
        return;
    };
    let video = place.surface.program;
    let off = place.surface_point((video.x + video.w / 2.0, video.y + video.h / 2.0));
    let before = env.shared.playhead();
    surface::post_press(env.surface_hwnd, from, off);
    surface::post_release(env.surface_hwnd, off);
    std::thread::sleep(SETTLE);
    let after = env.shared.playhead();
    log.step(
        "release_off_button",
        (after - before).abs() < 1e-9,
        json!({ "before": before, "after": after }),
    );
}

/// Program のジョグバーのクリックとドラッグ。
fn jog_steps(env: &Env<'_>, log: &mut Log, place: &Placement) {
    let (shared, duration) = (env.shared, env.info.duration);
    let tolerance = jog_tolerance(place, Viewer::Program, duration, env.info.fps);
    let reached = |t: f64| wait_until(WAIT, || (shared.playhead() - t).abs() <= tolerance);
    let click = jog_at(place, Viewer::Program, JOG_CLICK);
    if let Some((x, y)) = click {
        surface::post_click(env.surface_hwnd, x, y);
    }
    let target = duration * f64::from(JOG_CLICK);
    let clicked = click.is_some() && reached(target);
    log.step(
        "jog_click",
        clicked,
        json!({ "target": target, "playhead": shared.playhead(), "tolerance": tolerance }),
    );
    let ends = jog_at(place, Viewer::Program, JOG_FROM).zip(jog_at(place, Viewer::Program, JOG_TO));
    if let Some((from, to)) = ends {
        surface::post_drag(env.surface_hwnd, from, to);
    }
    let target = duration * f64::from(JOG_TO);
    let dragged = ends.is_some() && reached(target);
    log.step(
        "jog_drag",
        dragged,
        json!({ "target": target, "playhead": shared.playhead(), "tolerance": tolerance }),
    );
}

/// 最後の素材を Source に開き、Source のバーが出るまで待つ。
fn open_source(env: &Env<'_>, log: &mut Log) -> Option<Arc<SourceHandle>> {
    let shared = env.shared;
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let asset = doc.project.assets.last()?.id.to_string();
    let opened = shared.source.open(&doc.project, comp, &asset).is_ok();
    let handle = shared.source.handle();
    let shown = handle.as_ref().is_some_and(|h| {
        wait_until(WAIT, || {
            h.shared.frames.newer_than(0).is_some() && shared.source.is_shown()
        })
    });
    log.step("source_open", opened && shown, json!({ "asset": asset }));
    handle.filter(|_| opened && shown)
}

/// Source のジョグバーで位置を決めて In と Out のボタンを押し、イン点とアウト点が付くのを待つ。
fn mark_steps(env: &Env<'_>, place: &Placement, h: &SourceHandle) -> bool {
    let fps = h.fps.to_f64();
    let tolerance = jog_tolerance(place, Viewer::Source, h.duration, fps) + 1.0 / fps;
    let mut expected = Vec::new();
    for (frac, id) in [
        (MARK_IN_AT, "source.mark_in"),
        (MARK_OUT_AT, "source.mark_out"),
    ] {
        let Some((x, y)) = jog_at(place, Viewer::Source, frac) else {
            return false;
        };
        surface::post_click(env.surface_hwnd, x, y);
        let t = h.duration * f64::from(frac);
        wait_until(WAIT, || (h.shared.playhead() - t).abs() <= tolerance);
        if !press(env, place, Viewer::Source, id) {
            return false;
        }
        expected.push(t);
        std::thread::sleep(SETTLE);
    }
    let close = |v: Option<f64>, t: f64| v.is_some_and(|v| (v - t).abs() <= tolerance);
    wait_until(WAIT, || {
        let (i, o) = marks_of(env.shared, &h.asset);
        close(i, expected[0]) && close(o, expected[1])
    })
}

/// Source のバーの In、Out、Clear のボタンと、Alt+X。
fn source_steps(env: &Env<'_>, log: &mut Log) {
    let Some(h) = open_source(env, log) else {
        return;
    };
    let Some(place) = current_place(env.shared, env.surface_hwnd) else {
        log.step("source_marks", false, json!("割り付けを引けない"));
        return;
    };
    let cleared = || wait_until(WAIT, || marks_of(env.shared, &h.asset) == (None, None));
    if marks_of(env.shared, &h.asset) != (None, None) {
        press(env, &place, Viewer::Source, "source.clear_in_out");
        cleared();
    }
    let marked = mark_steps(env, &place, &h);
    let marks = marks_of(env.shared, &h.asset);
    log.step("source_marks", marked, json!({ "marks": marks }));
    shot(env, log, "source");
    let clear = press(env, &place, Viewer::Source, "source.clear_in_out") && cleared();
    log.step(
        "source_clear",
        marked && clear,
        json!({ "marks": marks_of(env.shared, &h.asset) }),
    );
    alt_x_step(env, log, &place, &h);
}

/// イン点とアウト点を付け直し、Source の映像の領域をクリックしてサーフェスにフォーカスを置き、Alt+X を `SendInput` で押す。
fn alt_x_step(env: &Env<'_>, log: &mut Log, place: &Placement, h: &SourceHandle) {
    if !mark_steps(env, place, h) {
        log.step("alt_x", false, json!("イン点とアウト点を付けられない"));
        return;
    }
    if let Some(video) = place.surface.source {
        let (x, y) = place.surface_point((video.x + video.w / 2.0, video.y + video.h / 2.0));
        surface::post_click(env.surface_hwnd, x, y);
        std::thread::sleep(SETTLE);
    }
    if unsafe { GetForegroundWindow() }.0 as isize != env.window_hwnd {
        log.skip(
            "alt_x",
            "ウィンドウが前面でない（ほかのアプリにキーを送らない）",
        );
        return;
    }
    let sent = send_alt_x();
    let cleared = wait_until(WAIT, || marks_of(env.shared, &h.asset) == (None, None));
    log.step(
        "alt_x",
        sent == 4 && cleared,
        json!({ "sent_inputs": sent }),
    );
}

/// Alt を押し、X を押して離し、Alt を離す。送れた入力の数を返す。
fn send_alt_x() -> u32 {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(VK_MENU, false),
        key(VK_X, false),
        key(VK_X, true),
        key(VK_MENU, true),
    ];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) }
}

/// 再生の操作のバーの通しを行って結果を返す。Program のバーを出していなければスキップする。
pub fn run(env: &Env<'_>) -> Value {
    let mut log = Log::default();
    let Some(place) = current_place(env.shared, env.surface_hwnd)
        .filter(|p| p.jog_track(Viewer::Program).is_some())
    else {
        return json!({ "skipped": "Program のバーが出ていない" });
    };
    program_steps(env, &mut log, &place);
    release_off_step(env, &mut log, &place);
    jog_steps(env, &mut log, &place);
    source_steps(env, &mut log);
    log.into_json()
}
