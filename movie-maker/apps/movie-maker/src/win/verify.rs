//! `--play-test` の自動の再生試験。プロジェクトを先頭から最後まで再生して、ドロップしたフレームの数、
//! 映像と音のずれ、WebView の時計の誤差を測り、途中のスクリーンショットと一緒に残して終わる。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_trace::chrome::ChromeTraceHandle;
use mm_trace::MetricsHandle;
use mm_types::ParamValue;
use mm_ui_layout::{
    row_top, split_viewers, Scale, SurfaceLayout, TimeView, TimelineModel, ROW_H, RULER_H,
    VIEW_MARGIN,
};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Window};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::keys;
use crate::win::playback::{Command, PlaybackInfo};
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_edit;

/// 描画が安定して WebView の初期化が終わるまでの待ち。
const SETTLE: Duration = Duration::from_millis(2500);
/// 再生が始まるのを待つ上限。
const START_TIMEOUT: Duration = Duration::from_secs(5);
/// 再生の時間を超えて待つ余裕。
const FINISH_SLACK: Duration = Duration::from_secs(5);
/// 状態を見に行く間隔。
const POLL: Duration = Duration::from_millis(25);

/// スクラブで狙う時刻（秒）。
const SCRUB_TARGET: f64 = 3.0;
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

/// 試験の対象。ウィンドウと、サーフェスの子 HWND と、Timeline のモデル。
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

/// 試験のスレッドを始める。終わったらアプリを閉じる。
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
            let (target, test) = (Arc::new(target), Arc::new(test));
            let report = run(&app, &target, &shared, &info, &telemetry, &test);
            let text = serde_json::to_string_pretty(&report).unwrap_or_default();
            if let Err(e) = std::fs::write(&test.report, text) {
                eprintln!("結果を書けない: {e}");
            }
            write_telemetry(&telemetry, &test);
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

pub(crate) fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit && !cancelled() {
        if done() {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

/// `Ui` と同じ `split_viewers` で計算した論理ピクセルのサーフェスの割り付けと、表示の倍率。クリックを投げるときは
/// 倍率を掛けて物理ピクセルに直す。Source の有無はフレームの到着から 1 周遅れて変わる `Ui` の側の値
/// （`SourceSlot::is_shown`）を読む。
pub(crate) fn viewer_layout(
    shared: &Shared,
    hwnd: isize,
    timeline_content_h: f32,
) -> (SurfaceLayout, Scale) {
    let scale = Scale::new(surface::scale_factor(hwnd));
    let (w, h) = shared.surface_size();
    let layout = split_viewers(
        scale.to_logical(w as f32),
        scale.to_logical(h as f32),
        timeline_content_h,
        shared.source.is_shown(),
    );
    (layout, scale)
}

/// 論理ピクセルの点を、サーフェスのクライアントの物理ピクセルにする。
/// 切り捨てると最大 1 物理ピクセル左上へ寄るので、近い画素に丸める。
pub(crate) fn client_point(scale: Scale, (x, y): (f32, f32)) -> (i32, i32) {
    let p = |v: f32| (v * scale.get()).round() as i32;
    (p(x), p(y))
}

/// 段ごとの上限。超えた段は report に失敗として書き、次へ進む。
const INTERACTION_LIMIT: Duration = Duration::from_secs(20);
const INSPECTOR_LIMIT: Duration = Duration::from_secs(30);
const EDIT_LIMIT: Duration = Duration::from_secs(90);
/// 再生を止めてから止まった表示になるまでの待ち。
const PAUSE_WAIT: Duration = Duration::from_secs(2);
/// 打ち切った段のスレッドが終わるのを待つ余裕。以降の後始末と編集を取り合わせない。
const CANCEL_GRACE: Duration = Duration::from_secs(10);
/// 編集を戻すために取り消しを送る回数の上限と、1 回ごとの待ち。
const RESTORE_MAX_UNDO: usize = 8;
const RESTORE_WAIT: Duration = Duration::from_secs(2);

thread_local! {
    /// 段のスレッドを打ち切る合図。立つと `wait_until` が待たずに偽を返し、段が速やかに終わる。
    static CANCEL: std::cell::RefCell<Option<Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) };
}

fn cancelled() -> bool {
    CANCEL.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|f| f.load(Ordering::SeqCst))
    })
}

/// 段を別のスレッドで走らせ、上限までに返らなければ打ち切る。ウィンドウへの呼び出しなどで段が固まっても、
/// 試験全体は進んで report を書ける。
fn stage<T: Send + 'static>(
    name: &str,
    limit: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let thread_flag = flag.clone();
    std::thread::Builder::new()
        .name(format!("play-test-{name}"))
        .spawn(move || {
            CANCEL.with(|c| *c.borrow_mut() = Some(thread_flag));
            let _ = tx.send(f());
        })
        .map_err(|e| format!("段を始められない: {e}"))?;
    match rx.recv_timeout(limit) {
        Ok(v) => Ok(v),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err("段が異常終了した".into()),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            flag.store(true, Ordering::SeqCst);
            let _ = rx.recv_timeout(CANCEL_GRACE);
            Err(format!("上限 {} 秒で打ち切った", limit.as_secs()))
        }
    }
}

/// 段の結果を report の値にする。打ち切りや異常は失敗として書き、`failed` に段の名前を積む。
fn settle(name: &str, result: Result<Value, String>, failed: &mut Vec<String>) -> Value {
    result.unwrap_or_else(|e| {
        eprintln!("[play-test] {name}: {e}");
        failed.push(name.to_owned());
        json!({ "ok": false, "failed": e })
    })
}

/// 試験が当てた編集を、試験の前のプロジェクトと同じになるまで取り消す。途中で打ち切った段が編集を残しても、
/// 開いたプロジェクトに跡を残さない。
fn restore_project(shared: &Arc<Shared>, baseline: Option<Arc<mm_doc::Project>>) -> Value {
    let Some(baseline) = baseline else {
        return json!({ "restored": true, "undos": 0 });
    };
    let same = || shared.doc().is_some_and(|d| *d.project == *baseline);
    let mut undos = 0;
    while !same() && undos < RESTORE_MAX_UNDO {
        let before = shared.doc().map(|d| Arc::as_ptr(&d.project));
        keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
        undos += 1;
        if !wait_until(RESTORE_WAIT, || {
            shared.doc().map(|d| Arc::as_ptr(&d.project)) != before
        }) {
            break;
        }
    }
    json!({ "restored": same(), "undos": undos })
}

fn run(
    app: &AppHandle,
    target: &Arc<Target>,
    shared: &Arc<Shared>,
    info: &PlaybackInfo,
    telemetry: &Telemetry,
    test: &Arc<PlayTest>,
) -> Value {
    let window = &target.window;
    let hwnd = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    wait_until(Duration::from_secs(20), || {
        shared.frames_presented.load(Ordering::SeqCst) > 30
            && !shared.adapter.lock().expect("adapter").is_empty()
    });
    std::thread::sleep(SETTLE);
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    shared.send(Command::Seek(0.0));
    std::thread::sleep(POLL * 4);
    let _ = shared.drain_series();
    let base = Baseline::take(shared);
    let _ = telemetry.metrics.drain();
    let baseline_project = shared.doc().map(|d| d.project);
    let mut failed = Vec::new();

    shared.send(Command::Play);
    let started = wait_until(START_TIMEOUT, || shared.playing.load(Ordering::SeqCst));
    let t0 = Instant::now();
    let play_for = Duration::from_secs_f64(info.duration.min(test.secs));
    let played = {
        let (s, i, t) = (shared.clone(), info.clone(), test.clone());
        stage("play", play_for + FINISH_SLACK * 2, move || {
            play_and_capture(&s, &i, &t, hwnd, t0, play_for)
        })
    };
    let wall = t0.elapsed().as_secs_f64();
    let (shot, shot_at) = played.unwrap_or_else(|e| {
        eprintln!("[play-test] play: {e}");
        failed.push("play".to_owned());
        (None, Value::Null)
    });
    let mut report = report(shared, info, test, &base, started, wall, shot);
    report["screenshot_at"] = shot_at;
    shared.send(Command::Pause);
    wait_until(PAUSE_WAIT, || !shared.playing.load(Ordering::SeqCst));

    let r = {
        let (s, i, t) = (shared.clone(), info.clone(), target.clone());
        stage("interaction", INTERACTION_LIMIT, move || {
            interaction(&s, &i, &t)
        })
    };
    report["interaction"] = settle("interaction", r, &mut failed);
    let r = {
        let (a, s, t, p) = (app.clone(), shared.clone(), target.clone(), test.clone());
        stage("inspector", INSPECTOR_LIMIT, move || {
            inspector_flow(&a, &s, &t, hwnd, &p)
        })
    };
    report["inspector"] = settle("inspector", r, &mut failed);
    let r = {
        let (s, t, p) = (shared.clone(), target.clone(), test.clone());
        stage("timeline_edit", EDIT_LIMIT, move || {
            verify_edit::run(&verify_edit::Env {
                shared: &s,
                surface_hwnd: t.surface_hwnd,
                window_hwnd: hwnd,
                test: &p,
            })
        })
    };
    report["timeline_edit"] = settle("timeline_edit", r, &mut failed);
    report["restore"] = restore_project(shared, baseline_project);
    report["failed_stages"] = json!(failed);
    let _ = window.set_always_on_top(false);
    report
}

/// 再生が終わるか `limit`（要求の秒数）になるまで待ち、半分まで進んだところでスクリーンショットを撮る。撮った時の再生位置と、Program に
/// 渡した最新のフレームの時刻も返す。WebView の時刻の表示は画面への反映がサーフェスより約 2 フレーム遅れるので、
/// スクリーンショットの画像と比べるのはこちらの時刻にする。
fn play_and_capture(
    shared: &Shared,
    info: &PlaybackInfo,
    test: &PlayTest,
    hwnd: isize,
    t0: Instant,
    limit: Duration,
) -> (Option<String>, Value) {
    let mut shot = None;
    let mut at = Value::Null;
    let half = Duration::from_secs_f64((info.duration.min(test.secs) / 2.0).max(0.1));
    wait_until(limit, || {
        if shot.is_none() && t0.elapsed() >= half {
            at = json!({
                "playhead_secs": shared.playhead(),
                "frame_secs": shared.frames.newer_than(0).map(|f| f.time),
            });
            shot = test
                .screenshot
                .as_ref()
                .map(|path| match capture::capture_window(hwnd, path) {
                    Ok(()) => path.display().to_string(),
                    Err(e) => format!("失敗: {e}"),
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
    let total = (info.duration * info.fps).round();
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
    let (layout, scale) = viewer_layout(shared, target.surface_hwnd, target.model.content_height());
    let tl = layout.timeline;
    let view = TimeView::fit(target.model.duration, tl.w, VIEW_MARGIN);
    let (x, y) = client_point(
        scale,
        (tl.x + view.x_of(SCRUB_TARGET), tl.y + RULER_H / 2.0),
    );
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
        "scale_factor": scale.get(),
        "scrub_target_secs": SCRUB_TARGET,
        "scrub_result_secs": scrubbed,
        "scrub_ok": (scrubbed - SCRUB_TARGET).abs() <= 2.0 / info.fps,
        "space_starts_playback": !before && started,
        "space_stops_playback": stopped,
    })
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

fn shoot(hwnd: isize, test: &PlayTest, suffix: &str) -> Option<String> {
    let path = test
        .screenshot
        .as_ref()?
        .with_extension(format!("{suffix}.png"));
    capture::capture_window(hwnd, &path)
        .ok()
        .map(|()| path.display().to_string())
}

/// 実際のウィンドウで、Timeline のレイヤーを選ぶ → Inspector に出る → 値を変える → `apply_op` → 描き直し、の道を通す。
/// 選択と値の反映は共有状態で確かめ、Inspector の表示はスクリーンショットで目視する。
fn inspector_flow(
    app: &AppHandle,
    shared: &Shared,
    target: &Target,
    hwnd: isize,
    test: &PlayTest,
) -> Value {
    let (layout, scale) = viewer_layout(shared, target.surface_hwnd, target.model.content_height());
    let tl = layout.timeline;
    let view = TimeView::fit(target.model.duration, tl.w, VIEW_MARGIN);
    let Some(item) = target.model.items.first() else {
        return json!({ "skipped": "レイヤーが無い" });
    };
    let (x, y) = client_point(
        scale,
        (
            tl.x + view.x_of((item.start + item.end) / 2.0),
            tl.y + row_top(0) + ROW_H / 2.0,
        ),
    );
    surface::post_click(target.surface_hwnd, x, y);
    let selected = wait_until(FLOW_TIMEOUT, || shared.selected_layer().is_some());
    let id = shared.selected_layer();
    std::thread::sleep(INPUT_WAIT);
    let shot_selected = shoot(hwnd, test, "inspector-selected");
    let rendered_before = shared.rendered_frames.load(Ordering::SeqCst);
    if let Some(webview) = app.get_webview("ui") {
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
