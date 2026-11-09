//! `--play-test` の再生テスト。プロジェクトを先頭から要求の秒数だけ再生して、ドロップしたフレームの数、
//! 映像と音のずれ、WebView の時計の誤差を測り、途中のスクリーンショットと一緒に残す。続けて入力、Inspector、
//! Timeline の編集、Timeline のズームと移動の工程を通し、テストが適用した編集を戻してから結果の JSON を書いて終わる。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_trace::chrome::ChromeTraceHandle;
use mm_trace::MetricsHandle;
use mm_types::ParamValue;
use mm_ui_layout::{TimelineModel, HANDLE_PX};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Window};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::feedback::panic_message;
use crate::win::playback::{Command, PlaybackInfo};
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_edit;
use crate::win::verify_place::placement;
use crate::win::verify_restore::{self, Savepoint};
use crate::win::verify_stage::{self, checkpoint, wait_until, StageError};
use crate::win::verify_zoom;

/// 描画が安定して WebView の初期化が終わるまでの待ち。
const SETTLE: Duration = Duration::from_millis(2500);
/// 描画が始まったとみなす、画面に出したフレームの数と、それを待つ上限。
const READY_FRAMES: u64 = 30;
const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// 再生が始まるのを待つ上限。
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// 状態を見に行く間隔。
const POLL: Duration = Duration::from_millis(25);
/// スクリーンショットを撮る位置の下限。再生がごく短くても、始まってから撮る。
const SHOT_AT_MIN: Duration = Duration::from_millis(100);
/// 再生を止めてから、再生スレッドが止めたと返すまでの待ち。
const PAUSE_WAIT: Duration = Duration::from_secs(2);

/// 工程ごとの上限の時間。中の待ちが全部上限まで待っても収まる長さに余裕を足し、超えたら呼び出しが返らないとみなす。
/// 再生は再生する秒数にこれを足す。
const PLAY_SLACK: Duration = Duration::from_secs(10);
/// 入力は待ちの合計が約 1 秒。
const INTERACTION_LIMIT: Duration = Duration::from_secs(20);
/// Inspector は待ちの合計が約 8 秒。
const INSPECTOR_LIMIT: Duration = Duration::from_secs(30);
/// Timeline の編集は待ちの合計が約 75 秒。
const EDIT_LIMIT: Duration = Duration::from_secs(90);
/// Timeline のズームと移動は、全部の待ちが上限まで待つと約 70 秒（ふだんは数秒）。
const ZOOM_LIMIT: Duration = Duration::from_secs(90);

/// スクラブで狙う時刻（秒）。
const SCRUB_TARGET: f64 = 3.0;
/// スクラブの許容誤差の下限（フレーム）。
const SCRUB_MIN_FRAMES: f64 = 2.0;
/// スクラブの許容誤差に足す、浮動小数の誤差の分（秒）。
const SCRUB_EPS: f64 = 1e-9;
/// クリックで選ぶ帯に要る、見えている幅（論理ピクセル）。中央が端のつまみに掛からない幅。
const CLICK_MIN_PX: f32 = 3.0 * HANDLE_PX;
/// 入力を投げてから結果を見るまでの待ち。
const INPUT_WAIT: Duration = Duration::from_millis(400);
/// スペースキーの仮想キーコード。
const VK_SPACE: u32 = 0x20;
/// Inspector で入れる不透明度。
const INSPECTOR_OPACITY: f64 = 0.5;
/// 選択や値の変更が反映されるのを待つ上限。
const FLOW_TIMEOUT: Duration = Duration::from_secs(3);
/// WebView の Inspector の不透明度の入力欄に値を入れて change を投げる。
const SET_OPACITY_JS: &str = r#"(() => {
  const input = document.querySelector('.mm-shell-right [aria-label="opacity"]');
  if (!input) return;
  input.value = "0.5";
  input.dispatchEvent(new Event("change", { bubbles: true }));
})()"#;

/// テストの対象。ウィンドウと、サーフェスの子 HWND と、Timeline のモデル。
pub struct Target {
    pub window: Window,
    pub surface_hwnd: isize,
    pub model: TimelineModel,
}

/// 計測の出力先。
pub struct Telemetry {
    pub metrics: MetricsHandle,
    pub chrome: ChromeTraceHandle,
}

/// 工程のスレッドに渡す、テストの対象と設定。
#[derive(Clone)]
struct TestEnv {
    app: AppHandle,
    target: Arc<Target>,
    shared: Arc<Shared>,
    info: PlaybackInfo,
    test: Arc<PlayTest>,
    /// ウィンドウの HWND（スクリーンショットと表示の倍率に使う）。
    hwnd: isize,
}

/// 再生の前の数字。再生の区間だけを数えるために引く。
struct Baseline {
    dropped: u64,
    rendered: u64,
    presented: u64,
    underruns: u64,
}

impl Baseline {
    fn take(shared: &Shared) -> Self {
        Self {
            dropped: shared.dropped_frames.load(Ordering::SeqCst),
            rendered: shared.rendered_frames.load(Ordering::SeqCst),
            presented: shared.frames_presented.load(Ordering::SeqCst),
            underruns: shared.clock.get().map_or(0, |c| c.underruns()),
        }
    }
}

/// 工程を順に実行し、結果を返さなかった工程の名前を集める。時間切れが出たら、後の工程は実行しない
/// （返らない呼び出しはウィンドウが応答しないときに起き、後の工程も同じところで止まるため）。
#[derive(Default)]
struct Stages {
    failed: Vec<String>,
    timed_out: bool,
}

impl Stages {
    /// 工程 `name` を実行する。実行しなかったか結果を返さなかったら、結果の JSON に書く値を `Err` で返す。
    fn try_run<T: Send + 'static>(
        &mut self,
        name: &str,
        limit: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Value> {
        if self.timed_out {
            return Err(json!({ "skipped": "前の工程が時間切れ" }));
        }
        verify_stage::run(name, limit, f).map_err(|e| {
            eprintln!("[play-test] {name}: {e}");
            self.timed_out |= matches!(e, StageError::TimedOut(_));
            self.failed.push(name.to_owned());
            json!({ "ok": false, "failed": e.to_string() })
        })
    }

    /// 結果の JSON の値を返す工程を実行する。
    fn run(
        &mut self,
        name: &str,
        limit: Duration,
        f: impl FnOnce() -> Value + Send + 'static,
    ) -> Value {
        self.try_run(name, limit, f).unwrap_or_else(|v| v)
    }
}

/// テストのスレッドを始める。終わったらアプリを閉じる。
pub fn spawn(
    app: AppHandle,
    target: Target,
    shared: Arc<Shared>,
    info: PlaybackInfo,
    telemetry: Telemetry,
    test: PlayTest,
) {
    std::thread::Builder::new()
        .name("play-test".into())
        .spawn(move || {
            let env = TestEnv {
                app: app.clone(),
                hwnd: target.window.hwnd().map(|h| h.0 as isize).unwrap_or(0),
                target: Arc::new(target),
                shared,
                info,
                test: Arc::new(test),
            };
            let report = catch_unwind(AssertUnwindSafe(|| run(&env, &telemetry))).unwrap_or_else(
                |p| json!({ "failed": format!("パニックした: {}", panic_message(&*p)) }),
            );
            let text = serde_json::to_string_pretty(&report).unwrap_or_default();
            if let Err(e) = std::fs::write(&env.test.report, text) {
                eprintln!("結果を書けない: {e}");
            }
            write_telemetry(&telemetry, &env.test);
            app.exit(0);
        })
        .expect("play-test thread");
}

fn write_telemetry(t: &Telemetry, test: &PlayTest) {
    let line = t.metrics.snapshot().to_jsonl_line();
    let _ = std::fs::write(test.report.with_extension("metrics.jsonl"), line + "\n");
    let mut trace = Vec::new();
    if t.chrome.write_to(&mut trace).is_ok() {
        let _ = std::fs::write(test.report.with_extension("trace.json"), trace);
    }
}

fn run(env: &TestEnv, telemetry: &Telemetry) -> Value {
    let shared = &env.shared;
    prepare(env);
    let _ = shared.drain_series();
    let base = Baseline::take(shared);
    let _ = telemetry.metrics.drain();
    let savepoint = Savepoint::take(shared);
    let mut stages = Stages::default();
    let mut report = play(env, &base, &mut stages);
    shared.send(Command::Pause);
    wait_until(PAUSE_WAIT, || !shared.playing.load(Ordering::SeqCst));

    let e = env.clone();
    report["interaction"] = stages.run("interaction", INTERACTION_LIMIT, move || {
        interaction(&e.shared, &e.info, &e.target)
    });
    let e = env.clone();
    report["inspector"] = stages.run("inspector", INSPECTOR_LIMIT, move || inspector_flow(&e));
    let e = env.clone();
    report["timeline_edit"] = stages.run("timeline_edit", EDIT_LIMIT, move || timeline_edit(&e));
    let e = env.clone();
    report["timeline_zoom"] = stages.run("timeline_zoom", ZOOM_LIMIT, move || timeline_zoom(&e));
    report["restore"] = verify_restore::restore(savepoint.as_ref(), shared);
    report["failed_stages"] = json!(stages.failed);
    let _ = env.target.window.set_always_on_top(false);
    report
}

/// 描画が始まって WebView の初期化が終わるのを待ち、ウィンドウを前に出して先頭へシークする。
fn prepare(env: &TestEnv) {
    let (shared, window) = (&env.shared, &env.target.window);
    wait_until(READY_TIMEOUT, || {
        shared.frames_presented.load(Ordering::SeqCst) > READY_FRAMES
            && !shared.adapter.lock().expect("adapter").is_empty()
    });
    std::thread::sleep(SETTLE);
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    shared.send(Command::Seek(0.0));
    std::thread::sleep(POLL * 4);
}

/// 再生する秒数。作品が要求の秒数より短ければ作品の長さ。
fn played_secs(info: &PlaybackInfo, test: &PlayTest) -> f64 {
    info.duration.min(test.secs).max(0.0)
}

/// 再生の工程。再生して数を数え、半分の位置でスクリーンショットを撮る。
fn play(env: &TestEnv, base: &Baseline, stages: &mut Stages) -> Value {
    let shared = &env.shared;
    shared.send(Command::Play);
    let started = wait_until(START_TIMEOUT, || shared.playing.load(Ordering::SeqCst));
    let t0 = Instant::now();
    let span = Duration::from_secs_f64(played_secs(&env.info, &env.test));
    let e = env.clone();
    let played = stages.try_run("play", span + PLAY_SLACK, move || {
        play_and_capture(&e, t0, span)
    });
    let wall = t0.elapsed().as_secs_f64();
    let ((shot, shot_at), failure) = match played {
        Ok(captured) => (captured, None),
        Err(failure) => ((None, Value::Null), Some(failure)),
    };
    let mut report = report(shared, &env.info, &env.test, base, started, wall, shot);
    report["screenshot_at"] = shot_at;
    if let Some(failure) = failure {
        report["play"] = failure;
    }
    report
}

/// 再生が終わるか `span`（再生する秒数）が過ぎるまで待ち、半分まで進んだところでスクリーンショットを撮る。撮った時の
/// 再生位置と、Program に渡した最新のフレームの時刻も返す。WebView の時刻の表示は画面への反映がサーフェスより
/// 約 2 フレーム遅れるので、スクリーンショットの画像と比べるのはこちらの時刻にする。
fn play_and_capture(env: &TestEnv, t0: Instant, span: Duration) -> (Option<String>, Value) {
    let shared = &env.shared;
    let mut shot = None;
    let mut at = Value::Null;
    let half = (span / 2).max(SHOT_AT_MIN);
    wait_until(span, || {
        if shot.is_none() && t0.elapsed() >= half {
            at = json!({
                "playhead_secs": shared.playhead(),
                "frame_secs": shared.frames.newer_than(0).map(|f| f.time),
            });
            shot = env.test.screenshot.as_ref().map(|path| {
                match capture::capture_window(env.hwnd, path) {
                    Ok(()) => path.display().to_string(),
                    Err(e) => format!("失敗: {e}"),
                }
            });
            if shot.is_none() {
                shot = Some(String::new());
            }
        }
        !shared.playing.load(Ordering::SeqCst)
    });
    (shot.filter(|s| !s.is_empty()), at)
}

fn report(
    shared: &Shared,
    info: &PlaybackInfo,
    test: &PlayTest,
    base: &Baseline,
    started: bool,
    wall: f64,
    shot: Option<String>,
) -> Value {
    let now = Baseline::take(shared);
    let series = shared.drain_series();
    let dropped = now.dropped - base.dropped;
    let rendered = now.rendered - base.rendered;
    let presented = now.presented - base.presented;
    let total = (played_secs(info, test) * info.fps).round();
    json!({
        "play_started": started,
        "final_position_secs": shared.playhead(),
        "playing_at_end": shared.playing.load(Ordering::SeqCst),
        "requested_secs": test.secs,
        "content_secs": info.duration,
        "wall_secs": wall,
        "clock": info.clock,
        "audio_device": info.audio_device,
        "adapter": shared.adapter.lock().expect("adapter").clone(),
        "backends": info.backends,
        "frames_total": total,
        "frames_rendered": rendered,
        "frames_dropped": dropped,
        "drop_rate_percent": if total > 0.0 { dropped as f64 / total * 100.0 } else { 0.0 },
        "frames_presented": presented,
        "present_fps": presented as f64 / wall.max(1e-9),
        "audio_underruns": now.underruns - base.underruns,
        "series": series,
        "screenshot": shot,
    })
}

/// 実際のウィンドウへの入力で、Timeline のスクラブとキー操作が効くか確かめる。
fn interaction(shared: &Shared, info: &PlaybackInfo, target: &Target) -> Value {
    let Some(place) = placement(shared, target.surface_hwnd, &target.model) else {
        return json!({ "skipped": "表示の状態がまだ無い" });
    };
    let (x, y) = place.ruler_point(SCRUB_TARGET);
    let tolerance = scrub_tolerance(place.px_per_sec() * f64::from(place.scale.get()), info.fps);
    surface::post_click(target.surface_hwnd, x, y);
    std::thread::sleep(INPUT_WAIT);
    let scrubbed = shared.playhead();
    let playing = || shared.playing.load(Ordering::SeqCst);
    let before = playing();
    surface::post_key(target.surface_hwnd, VK_SPACE);
    std::thread::sleep(INPUT_WAIT);
    let started = playing();
    surface::post_key(target.surface_hwnd, VK_SPACE);
    std::thread::sleep(INPUT_WAIT);
    let stopped = !playing();
    json!({
        "scale_factor": place.scale.get(),
        "scrub_target_secs": SCRUB_TARGET,
        "scrub_result_secs": scrubbed,
        "scrub_tolerance_secs": tolerance,
        "scrub_ok": (scrubbed - SCRUB_TARGET).abs() <= tolerance,
        "space_starts_playback": !before && started,
        "space_stops_playback": stopped,
    })
}

/// スクラブの許容誤差（秒）。クリックを物理ピクセルに丸めた半画素（1 秒が `phys_px_per_sec` 物理ピクセル）と、
/// シークをフレームに丸めた半フレームの和。長いコンポジションの全体表示は 1 物理ピクセルが 2 フレームより長いので、
/// 2 フレームに決めた許容誤差では、狙った所を押しても外れる。
fn scrub_tolerance(phys_px_per_sec: f64, fps: f64) -> f64 {
    let rounding = 0.5 / phys_px_per_sec + 0.5 / fps;
    (SCRUB_MIN_FRAMES / fps).max(rounding) + SCRUB_EPS
}

fn layer_opacity(shared: &Shared, id: &str) -> Option<f64> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let layer = comp.layers.iter().find(|l| l.id.as_str() == id)?;
    match layer.opacity.value {
        ParamValue::Number(n) => Some(n),
        _ => None,
    }
}

/// Inspector の工程で選ぶレイヤーの帯の中の点。行の領域に全部見えている行のうち、帯の中を掴める幅が見えている最初の行。
/// 開いたときは sequence の行（無ければいちばん下の行）までスクロールするので、先頭の行が見えているとは限らない。
fn layer_click(shared: &Shared, target: &Target) -> Option<(i32, i32)> {
    let place = placement(shared, target.surface_hwnd, &target.model)?;
    let span = |i: usize| (target.model.items[i].start, target.model.items[i].end);
    let row = place.first_visible(|i| place.bar_center(i, span(i), CLICK_MIN_PX).is_some())?;
    place.bar_center(row, span(row), CLICK_MIN_PX)
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
fn shoot(hwnd: isize, test: &PlayTest, suffix: &str) -> Option<String> {
    let path = test
        .screenshot
        .as_ref()?
        .with_extension(format!("{suffix}.png"));
    let shot = capture::capture_window(hwnd, &path)
        .ok()
        .map(|()| path.display().to_string());
    checkpoint();
    shot
}

/// 実際のウィンドウで、Timeline のレイヤーを選ぶ → Inspector に出る → 値を変える → `apply_op` → 描き直し、の道を通す。
/// 選択と値の反映は共有状態で確かめ、Inspector の表示はスクリーンショットで目視する。
fn inspector_flow(env: &TestEnv) -> Value {
    let (shared, target, hwnd, test) = (&*env.shared, &*env.target, env.hwnd, &*env.test);
    let Some((x, y)) = layer_click(shared, target) else {
        return json!({ "skipped": "掴める幅の帯が見えていない" });
    };
    surface::post_click(target.surface_hwnd, x, y);
    let selected = wait_until(FLOW_TIMEOUT, || shared.selected_layer().is_some());
    let id = shared.selected_layer();
    std::thread::sleep(INPUT_WAIT);
    let shot_selected = shoot(hwnd, test, "inspector-selected");
    let rendered_before = shared.rendered_frames.load(Ordering::SeqCst);
    if let Some(webview) = env.app.get_webview("ui") {
        let _ = webview.eval(SET_OPACITY_JS);
    }
    let applied = id.as_deref().is_some_and(|id| {
        wait_until(FLOW_TIMEOUT, || {
            layer_opacity(shared, id) == Some(INSPECTOR_OPACITY)
        })
    });
    std::thread::sleep(INPUT_WAIT);
    let rerendered = shared.rendered_frames.load(Ordering::SeqCst) > rendered_before;
    json!({
        "selected": selected,
        "selected_layer": id,
        "opacity_applied": applied,
        "rerendered_after_apply": rerendered,
        "screenshot_selected": shot_selected,
        "screenshot_applied": shoot(hwnd, test, "inspector-applied"),
    })
}

/// Timeline の編集の工程（`verify_edit`）。
fn timeline_edit(env: &TestEnv) -> Value {
    verify_edit::run(&verify_edit::Env {
        shared: &env.shared,
        surface_hwnd: env.target.surface_hwnd,
        window_hwnd: env.hwnd,
        test: &env.test,
    })
}

/// Timeline のズームと移動の工程（`verify_zoom`）。
fn timeline_zoom(env: &TestEnv) -> Value {
    verify_zoom::run(&verify_zoom::Env {
        shared: &env.shared,
        surface_hwnd: env.target.surface_hwnd,
        window_hwnd: env.hwnd,
        test: &env.test,
        webview: env.app.get_webview("ui"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn スクラブの許容誤差は_1_物理ピクセルが長い全体表示で広がり_下限は_2_フレーム() {
        // 9:38 の全体表示（1 秒が約 1.25 物理ピクセル）。半画素は 0.4 秒。
        let wide = scrub_tolerance(1.25, 30.0);
        assert!((wide - (0.4 + 0.5 / 30.0)).abs() < 1e-6, "{wide}");
        // 1 秒 125 物理ピクセルなら 2 フレーム。ちょうど 2 フレームずれた結果も許す。
        let zoomed = scrub_tolerance(125.0, 30.0);
        assert!(
            (2.0 / 30.0..2.0 / 30.0 + 1e-6).contains(&zoomed),
            "{zoomed}"
        );
        assert!((3.066_666_666_666_667_f64 - 3.0).abs() <= zoomed);
    }
}
