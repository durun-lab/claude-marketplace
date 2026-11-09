//! `--play-test` の Timeline のズームと移動の通し。実際のウィンドウへのキーとドラッグで、キーの拡大縮小（JIS の配列の
//! Shift+- を含む）、全体表示の行き来（Shift+Z）、ズームバーのドラッグ、前後の帯の端と 1 秒の移動、キーの押し続け、
//! 再生中のページ送りを順に行い、そのたびに表示の状態と再生位置が期待どおり変わったかを確かめて、段ごとにスクリーンショットを残す。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use mm_ui_layout::{edit_target, TimeView, TimelineModel, VIEW_MARGIN};
use mm_ui_wgpu::timeline_model;
use serde_json::{json, Value};
use tauri::Webview;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::keys;
use crate::win::playback::Command;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_place::{placement, Placement};
use crate::win::verify_stage::{checkpoint, wait_until};

/// 仮想キーコード。
const VK_SPACE: u32 = 0x20;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_OEM_MINUS: u32 = 0xBD;
/// 押し続けの繰り返しの WM_KEYDOWN の lParam（直前も押されていたことを示す 30 ビット目と、繰り返しの数 1）。
const REPEAT_LPARAM: isize = (1 << 30) | 1;
/// JIS の配列で ^ のキー（US の配列では '）。配列が JIS かを見分ける。
const VK_OEM_7: u32 = 0xDE;
/// 入力を投げてから結果を見るまでの待ち。
const SETTLE: Duration = Duration::from_millis(300);
/// 結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// ページ送りを確かめる前に拡大する、見えている範囲の長さの上限（秒）。再生して右端を越えるまでの待ちを短くする。
const PAGE_SPAN: f64 = 3.0;
/// ページ送りを確かめるとき、右端のこの秒だけ手前から再生する。
const PAGE_LEAD: f64 = 0.5;
/// ズームバーのスクロールボックスを引く量と、端を引く量（論理ピクセル）。
const THUMB_DRAG: f32 = 80.0;
const EDGE_DRAG: f32 = 40.0;
/// 端を掴むための、スクロールボックスの縁から内側へのずれ（論理ピクセル）。
const EDGE_INSET: f32 = 3.0;
/// 倍率と位置を比べるときの誤差（相対）。
const EPS: f64 = 1e-6;

/// 試験の対象。
pub struct Env<'a> {
    pub shared: &'a Arc<Shared>,
    pub surface_hwnd: isize,
    pub window_hwnd: isize,
    pub test: &'a PlayTest,
    /// 中央の列（`web/transport.js`）を持つ WebView。キーの名前を WebView の側でも確かめる。
    pub webview: Option<Webview>,
}

/// JIS の配列の Shift+-（押された文字は =、キーの位置は Minus）と、- の keydown を WebView に投げる。
const WEBVIEW_JIS_EQUAL: &str = r#"window.dispatchEvent(new KeyboardEvent("keydown", { key: "=", code: "Minus", shiftKey: true }))"#;
const WEBVIEW_MINUS: &str =
    r#"window.dispatchEvent(new KeyboardEvent("keydown", { key: "-", code: "Minus" }))"#;
/// 押し続けの繰り返しの - と Space の keydown を WebView に投げる。
const WEBVIEW_REPEAT_MINUS: &str = r#"window.dispatchEvent(new KeyboardEvent("keydown", { key: "-", code: "Minus", repeat: true }))"#;
const WEBVIEW_REPEAT_SPACE: &str = r#"window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", repeat: true }))"#;

/// 結果の記録。段ごとの合否と、撮ったスクリーンショット。
#[derive(Default)]
struct Log {
    steps: Vec<(String, Value)>,
    shots: Vec<String>,
}

impl Log {
    fn step(&mut self, name: &str, ok: bool, detail: Value) -> bool {
        self.steps
            .push((name.to_owned(), json!({ "ok": ok, "detail": detail })));
        ok
    }

    fn into_json(self) -> Value {
        let all_ok = self.steps.iter().all(|(_, v)| v["ok"] == true);
        let steps: serde_json::Map<String, Value> = self.steps.into_iter().collect();
        json!({ "all_ok": all_ok, "steps": steps, "screenshots": self.shots })
    }
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
fn shot(env: &Env<'_>, log: &mut Log, name: &str) {
    let Some(base) = &env.test.screenshot else {
        return;
    };
    std::thread::sleep(SETTLE);
    let path = base.with_extension(format!("zoom-{name}.png"));
    match capture::capture_window(env.window_hwnd, &path) {
        Ok(()) => log.shots.push(path.display().to_string()),
        Err(e) => log.shots.push(format!("失敗: {e}")),
    }
    checkpoint();
}

fn model(shared: &Shared) -> Option<TimelineModel> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    Some(timeline_model(&doc.project, comp))
}

/// 描画スレッドが置いた今の表示。
fn view(shared: &Shared) -> Option<TimeView> {
    shared.view.current().map(|v| v.time)
}

/// 表示が `want` を満たすまで待ち、そのときの表示を返す（満たさなければ最後の表示）。
fn wait_view(shared: &Shared, want: impl Fn(&TimeView) -> bool) -> Option<TimeView> {
    wait_until(WAIT, || view(shared).is_some_and(|v| want(&v)));
    view(shared)
}

/// 再生位置を `t` へ動かして待つ。
fn seek(shared: &Shared, t: f64) -> bool {
    shared.send(Command::Seek(t));
    wait_until(WAIT, || (shared.playhead() - t).abs() < 0.05)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS * a.abs().max(b.abs()).max(1.0)
}

/// 全体表示から始める。開いたときに `.mm/view.json` の拡大した表示を戻していても、倍率の比べ方をそろえる。
fn start_from_fit(shared: &Arc<Shared>, hwnd: isize, duration: f64) {
    let Some(width) = row_width(shared, hwnd) else {
        return;
    };
    let fit = TimeView::fit(duration, width, VIEW_MARGIN);
    if view(shared) != Some(fit) {
        keys::dispatch(shared, "play-test", "Shift+KeyZ");
        wait_view(shared, |v| *v == fit);
    }
}

/// 全体表示からキーで 2 回拡大する。再生位置の x は動かず、倍率は 2 倍になる。
fn zoom_keys_step(env: &Env<'_>, log: &mut Log, duration: f64) -> Option<TimeView> {
    let shared = env.shared;
    start_from_fit(shared, env.surface_hwnd, duration);
    let at = duration / 3.0;
    seek(shared, at);
    let before = view(shared)?;
    for _ in 0..2 {
        keys::dispatch(shared, "play-test", "=");
    }
    let after = wait_view(shared, |v| close(v.px_per_sec, before.px_per_sec * 2.0))?;
    let still = (after.x_of(at) - before.x_of(at)).abs() < 1.0;
    let doubled = close(after.px_per_sec, before.px_per_sec * 2.0);
    log.step(
        "zoom_in_keys",
        still && doubled,
        json!({ "before": before.px_per_sec, "after": after.px_per_sec, "playhead_x_kept": still }),
    );
    shot(env, log, "1-in");
    Some(after)
}

/// `-` のキーをサーフェスに投げて縮小し、JIS の配列の Shift+-（`=`）で拡大して戻す。記号のキーは今の配列で文字にする。
fn jis_keys_step(env: &Env<'_>, log: &mut Log, start: TimeView) {
    let shared = env.shared;
    let layout = surface::layout_of(env.surface_hwnd);
    let jis = surface::key_name_in(VK_OEM_7, (false, false, false), layout) == "^";
    let minus = surface::key_name_in(VK_OEM_MINUS, (false, false, false), layout);
    let shift_minus = surface::key_name_in(VK_OEM_MINUS, (false, true, false), layout);
    surface::post_key(env.surface_hwnd, VK_OEM_MINUS);
    let step = std::f64::consts::SQRT_2;
    let out = wait_view(shared, |v| close(v.px_per_sec * step, start.px_per_sec));
    let zoomed_out = out.is_some_and(|v| close(v.px_per_sec * step, start.px_per_sec));
    keys::dispatch(shared, "play-test", &shift_minus);
    let back = wait_view(shared, |v| close(v.px_per_sec, start.px_per_sec));
    let zoomed_in = back.is_some_and(|v| close(v.px_per_sec, start.px_per_sec));
    let ok = minus == "-" && zoomed_out && (!jis || (shift_minus == "=" && zoomed_in));
    log.step(
        "symbol_keys",
        ok,
        json!({
            "layout": format!("{:08X}", layout.0 as usize),
            "jis": jis,
            "minus": minus,
            "shift_minus": shift_minus,
            "minus_zoomed_out": zoomed_out,
            "shift_minus_zoomed_in": zoomed_in,
        }),
    );
}

/// WebView に JIS の配列の Shift+- と - の keydown を投げ、`transport.js` の `keyName` が記号を押された文字で引いて
/// 拡大と縮小になるかを確かめる。
fn webview_keys_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let (Some(webview), Some(start)) = (&env.webview, view(shared)) else {
        log.step("webview_symbol_keys", false, json!("WebView が無い"));
        return;
    };
    let _ = webview.eval(WEBVIEW_JIS_EQUAL);
    let zoomed_in = wait_view(shared, |v| v.px_per_sec > start.px_per_sec)
        .is_some_and(|v| v.px_per_sec > start.px_per_sec);
    let _ = webview.eval(WEBVIEW_MINUS);
    let back = wait_view(shared, |v| close(v.px_per_sec, start.px_per_sec))
        .is_some_and(|v| close(v.px_per_sec, start.px_per_sec));
    log.step(
        "webview_symbol_keys",
        zoomed_in && back,
        json!({ "shift_minus_zoomed_in": zoomed_in, "minus_zoomed_out": back }),
    );
}

/// Shift+Z で全体表示にし、もう一度で元の倍率と位置に戻す。
fn full_extent_step(env: &Env<'_>, log: &mut Log, duration: f64) {
    let shared = env.shared;
    let (Some(zoomed), Some(width)) = (view(shared), row_width(shared, env.surface_hwnd)) else {
        log.step("full_extent", false, json!("表示の状態が無い"));
        return;
    };
    keys::dispatch(shared, "play-test", "Shift+KeyZ");
    let whole = |v: &TimeView| {
        let (start, end) = v.visible(width);
        start <= 0.0 && end >= duration
    };
    let fit = wait_view(shared, whole);
    let fitted = fit.is_some_and(|v| whole(&v));
    shot(env, log, "2-full-extent");
    keys::dispatch(shared, "play-test", "Shift+KeyZ");
    let back = wait_view(shared, |v| *v == zoomed);
    log.step(
        "full_extent",
        fitted && back == Some(zoomed),
        json!({ "fit": fit.map(|v| v.px_per_sec), "back": back.map(|v| (v.origin, v.px_per_sec)) }),
    );
}

/// 行の領域の幅（論理ピクセル）。
fn row_width(shared: &Shared, hwnd: isize) -> Option<f32> {
    let m = model(shared)?;
    Some(placement(shared, hwnd, &m)?.viewport().layout.row_area.w)
}

fn place(shared: &Shared, hwnd: isize) -> Option<Placement> {
    placement(shared, hwnd, &model(shared)?)
}

/// ズームバーのスクロールボックスを右へ引いて表示を動かし、右端を左へ引いて拡大する。
fn zoombar_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let (Some(p), Some(before)) = (place(shared, hwnd), view(shared)) else {
        log.step("zoombar_drag", false, json!("ズームバーの位置を引けない"));
        return;
    };
    let thumb = p.zoom_thumb();
    let mid = thumb.x + thumb.w / 2.0;
    let want = before.origin + f64::from(THUMB_DRAG) * p.zoombar_secs_per_px();
    surface::post_drag(
        hwnd,
        p.zoombar_point(mid),
        p.zoombar_point(mid + THUMB_DRAG),
    );
    let moved = wait_view(shared, |v| v.origin != before.origin);
    let px_tolerance = 2.0 * p.zoombar_secs_per_px();
    let panned = moved.is_some_and(|v| {
        (v.origin - want).abs() <= px_tolerance && v.px_per_sec == before.px_per_sec
    });
    let edge = p.zoom_thumb().right() + THUMB_DRAG - EDGE_INSET;
    let start = moved.map_or(before.origin, |v| v.origin);
    surface::post_drag(
        hwnd,
        p.zoombar_point(edge),
        p.zoombar_point(edge - EDGE_DRAG),
    );
    let zoomed = wait_view(shared, |v| v.px_per_sec > before.px_per_sec);
    let edge_ok =
        zoomed.is_some_and(|v| v.px_per_sec > before.px_per_sec && close(v.origin, start));
    log.step(
        "zoombar_drag",
        panned && edge_ok,
        json!({
            "origin_before": before.origin, "origin_want": want, "origin_after": moved.map(|v| v.origin),
            "px_before": before.px_per_sec, "px_after_edge": zoomed.map(|v| v.px_per_sec),
        }),
    );
    shot(env, log, "3-zoombar");
}

/// ↓ と ↑ のキーをサーフェスに投げ、再生位置が次と前の帯の端に動くかを確かめる。
fn edit_keys_step(env: &Env<'_>, log: &mut Log, m: &TimelineModel) {
    let shared = env.shared;
    let from = shared.playhead();
    let next = edit_target(m, from, true);
    surface::post_key(env.surface_hwnd, VK_DOWN);
    let reached = next.is_some_and(|t| wait_until(WAIT, || (shared.playhead() - t).abs() < 1e-3));
    let previous = next.and_then(|t| edit_target(m, t, false));
    surface::post_key(env.surface_hwnd, VK_UP);
    let back = previous.is_some_and(|t| wait_until(WAIT, || (shared.playhead() - t).abs() < 1e-3));
    log.step(
        "edit_keys",
        reached && back,
        json!({ "from": from, "next": next, "previous": previous, "playhead": shared.playhead() }),
    );
}

/// Shift+→ と Shift+← で 1 秒（fps を丸めたフレームの数）進んで戻る。
fn second_keys_step(env: &Env<'_>, log: &mut Log, fps: f64) {
    let shared = env.shared;
    let from = shared.playhead();
    let second = fps.round() / fps;
    keys::dispatch(shared, "play-test", "Shift+ArrowRight");
    let forward = wait_until(WAIT, || {
        (shared.playhead() - (from + second)).abs() < 0.5 / fps
    });
    keys::dispatch(shared, "play-test", "Shift+ArrowLeft");
    let back = wait_until(WAIT, || (shared.playhead() - from).abs() < 0.5 / fps);
    log.step(
        "second_keys",
        forward && back,
        json!({ "from": from, "second": second, "playhead": shared.playhead() }),
    );
}

/// 押し続けの繰り返しの WM_KEYDOWN をサーフェスに投げる。
fn post_repeat(hwnd: isize, vk: u32) {
    let (wparam, lparam) = (WPARAM(vk as usize), LPARAM(REPEAT_LPARAM));
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_KEYDOWN, wparam, lparam);
    }
}

/// 表示が `start` から 1 回ぶん縮小されるのを待ち、`=` で戻す。縮小されたかを返す。
fn zoomed_out_once(shared: &Arc<Shared>, start: TimeView) -> bool {
    let out = |v: &TimeView| close(v.px_per_sec * std::f64::consts::SQRT_2, start.px_per_sec);
    let zoomed = wait_view(shared, out).is_some_and(|v| out(&v));
    keys::dispatch(shared, "play-test", "=");
    wait_view(shared, |v| close(v.px_per_sec, start.px_per_sec));
    zoomed
}

/// `send` で Space の繰り返しを投げた後も止まったままか。動き出していたら止める。
fn space_ignored(shared: &Shared, send: impl FnOnce()) -> bool {
    send();
    std::thread::sleep(SETTLE);
    let still = !shared.playing.load(Ordering::SeqCst);
    shared.send(Command::Pause);
    still
}

/// サーフェスへの押し続けの繰り返しで、→ は繰り返しのたびに 1 フレーム進み、- は縮小し、Space は再生を始めない。
/// WebView でも、`transport.js` が付けて送る `repeat` で同じになる。
fn repeat_keys_step(env: &Env<'_>, log: &mut Log, fps: f64) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let (from, Some(start)) = (shared.playhead(), view(shared)) else {
        log.step("repeat_keys", false, json!("表示の状態が無い"));
        return;
    };
    for _ in 0..3 {
        post_repeat(hwnd, VK_RIGHT);
    }
    let stepped = wait_until(WAIT, || {
        (shared.playhead() - from - 3.0 / fps).abs() < 0.5 / fps
    });
    post_repeat(hwnd, VK_OEM_MINUS);
    let zoomed = zoomed_out_once(shared, start);
    let still = space_ignored(shared, || post_repeat(hwnd, VK_SPACE));
    let webview = env.webview.as_ref().map(|w| {
        let _ = w.eval(WEBVIEW_REPEAT_MINUS);
        let zoomed = zoomed_out_once(shared, start);
        let still = space_ignored(shared, || {
            let _ = w.eval(WEBVIEW_REPEAT_SPACE);
        });
        (zoomed, still)
    });
    let ok = stepped && zoomed && still && webview == Some((true, true));
    log.step(
        "repeat_keys",
        ok,
        json!({ "from": from, "playhead": shared.playhead(), "zoomed_out": zoomed, "space_ignored": still, "webview": webview }),
    );
}

/// 見えている範囲が [`PAGE_SPAN`] 秒より短くなるまでキーで拡大する。
fn zoom_until_short(shared: &Arc<Shared>, width: f32) -> Option<TimeView> {
    for _ in 0..40 {
        let v = view(shared)?;
        if v.visible(width).1 - v.origin <= PAGE_SPAN {
            return Some(v);
        }
        keys::dispatch(shared, "play-test", "=");
        wait_view(shared, |n| n.px_per_sec > v.px_per_sec);
    }
    view(shared)
}

/// 拡大して右端の手前から再生し、再生位置が右端を越えたら見えている範囲が 1 画面ぶん送られるかを確かめる。
fn page_scroll_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let Some(width) = row_width(shared, env.surface_hwnd) else {
        log.step("page_scroll", false, json!("行の領域の幅を引けない"));
        return;
    };
    let Some(v) = zoom_until_short(shared, width) else {
        log.step("page_scroll", false, json!("表示の状態が無い"));
        return;
    };
    let (start, end) = v.visible(width);
    seek(shared, end - PAGE_LEAD);
    shared.send(Command::Play);
    let crossed = wait_until(WAIT, || shared.playhead() > end + PAGE_LEAD / 2.0);
    let paged = wait_view(shared, |n| n.origin > start);
    shared.send(Command::Pause);
    wait_until(WAIT, || !shared.playing.load(Ordering::SeqCst));
    let ok = crossed && paged.is_some_and(|n| close(n.origin, end) && n.px_per_sec == v.px_per_sec);
    log.step(
        "page_scroll",
        ok,
        json!({ "visible": [start, end], "origin_after": paged.map(|n| n.origin) }),
    );
    shot(env, log, "4-page-scroll");
}

/// ズームと移動の通しを行って結果を返す。
pub fn run(env: &Env<'_>) -> Value {
    let mut log = Log::default();
    let Some(m) = model(env.shared) else {
        return json!({ "skipped": "プロジェクトのデータが無い" });
    };
    if let Some(zoomed) = zoom_keys_step(env, &mut log, m.duration) {
        jis_keys_step(env, &mut log, zoomed);
    }
    webview_keys_step(env, &mut log);
    full_extent_step(env, &mut log, m.duration);
    zoombar_step(env, &mut log);
    edit_keys_step(env, &mut log, &m);
    second_keys_step(env, &mut log, m.fps);
    repeat_keys_step(env, &mut log, m.fps);
    page_scroll_step(env, &mut log);
    log.into_json()
}
