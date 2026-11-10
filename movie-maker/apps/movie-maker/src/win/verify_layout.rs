//! `--play-test` の割り付けの通し。実際のウィンドウへのマウスのメッセージで、縦の仕切り（WebView の幅）と横の仕切り
//! （Timeline の高さ）をドラッグし、割り付けと WebView の列が期待どおり変わったかと、つかんだまま Esc で押す前に戻るかを
//! 確かめて撮る。続けて割り付けを設定のファイルに書いて Reset UI Layout で既定に戻し、書いた設定を読み直して（再起動と
//! 同じ経路）同じ割り付けに戻るか、列を閉じたときサーフェスが広がり View のチェックが外れるか、閉じた列のパネルを選ぶ
//! コマンドがその列を開けるか、列を閉じたまま WebView を読み直しても閉じたままかも確かめる。`--play-test` はユーザーの
//! 本物の設定を読み書きしないので、設定のファイルは結果の隣に書く。

use std::sync::Arc;
use std::time::Duration;

use mm_ui_layout::{content_width, splitter_rect, timeline_height, SPLITTER_W};
use serde_json::{json, Map, Value};
use tauri::{Webview, Window};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::commands;
use crate::win::layout;
use crate::win::splitter;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_stage::{checkpoint, wait_until};
use crate::win::verify_webview::eval_json;
use crate::win::workspace::{self, Workspace};

/// 結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// スクリーンショットの前に、WebView が描き終えるのを待つ。
const PAINT_WAIT: Duration = Duration::from_millis(400);
/// 縦の仕切りを左へ動かす量（論理ピクセル。WebView がこの分広がる）。
const DRAG_WEB: f64 = 200.0;
/// 横の仕切りを上へ動かす量（論理ピクセル。Timeline がこの分高くなる）。
const DRAG_TIMELINE: f32 = 120.0;
/// 論理ピクセルの幅を比べるときの誤差。物理ピクセルへの丸めの分。
const TOL: f64 = 1.5;
/// Timeline の高さの割合の誤差。
const RATIO_TOL: f32 = 0.003;
/// WebView の 2 つの列の表示と幅（CSS ピクセル）。閉じた列は出ていない。
const DOM_JS: &str = r#"(() => {
  const info = (s) => {
    const el = document.querySelector(s);
    const shown = el !== null && getComputedStyle(el).display !== "none";
    return { shown, width: shown ? el.getBoundingClientRect().width : 0 };
  };
  return { inner: window.innerWidth, left: info(".mm-shell-left"), right: info(".mm-shell-right") };
})()"#;

/// テストの対象。
struct Env<'a> {
    shared: &'a Arc<Shared>,
    window: &'a Window,
    webview: &'a Webview,
    surface_hwnd: isize,
    window_hwnd: isize,
    test: &'a PlayTest,
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

    fn into_json(self) -> Value {
        let all_ok = self.steps.values().all(|v| v["ok"] != false);
        json!({ "all_ok": all_ok, "steps": self.steps, "screenshots": self.shots })
    }
}

impl Env<'_> {
    fn scale(&self) -> f64 {
        self.window.scale_factor().unwrap_or(1.0)
    }

    /// ウィンドウの幅（論理ピクセル）。
    fn window_w(&self) -> f64 {
        self.window
            .inner_size()
            .map_or(0.0, |s| f64::from(s.width) / self.scale())
    }

    /// サーフェスの大きさ（論理ピクセル）。
    fn surface(&self) -> (f32, f32) {
        let (w, h) = self.shared.surface_size();
        let s = self.scale() as f32;
        (w as f32 / s, h as f32 / s)
    }

    /// 割り付けから期待される WebView の幅（論理ピクセル）。
    fn web_w(&self) -> f64 {
        self.shared.workspace.get().columns(self.window_w()).web()
    }

    fn dom(&self) -> Value {
        eval_json(self.webview, DOM_JS)
    }

    fn shoot(&self, log: &mut Log, name: &str) {
        let Some(base) = &self.test.screenshot else {
            return;
        };
        std::thread::sleep(PAINT_WAIT);
        let path = base.with_extension(format!("layout-{name}.png"));
        match capture::capture_window(self.window_hwnd, &path) {
            Ok(()) => log.shots.push(path.display().to_string()),
            Err(e) => log.shots.push(format!("失敗: {e}")),
        }
        checkpoint();
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

/// 列の幅が `left` と `right`（0 は閉じている）になるまで待ち、WebView の中の列の幅と合うか。
fn columns_match(env: &Env<'_>, left: f64, right: f64) -> (bool, Value) {
    let mut dom = Value::Null;
    let ok = wait_until(WAIT, || {
        dom = env.dom();
        let w = |k: &str| dom[k]["width"].as_f64().unwrap_or(f64::NAN);
        close(w("left"), left) && close(w("right"), right)
    });
    (ok, dom)
}

/// 縦の仕切り（サーフェスの右端の幅 `SPLITTER_W`）の真ん中の点（物理ピクセル）。
fn vertical_grip(env: &Env<'_>) -> (i32, i32) {
    let s = env.scale();
    let half = (f64::from(SPLITTER_W) / 2.0 * s) as i32;
    let y = (f64::from(env.surface().1) / 3.0 * s) as i32;
    (env.shared.surface_size().0 as i32 - half, y)
}

/// 縦の仕切りをつかんで左へ動かし（動かしている間を撮る）、Esc で取り消すと押す前に戻る。続けて押す、動かす、離すを
/// 投げると、WebView が広がり、左の列だけが広がり、サーフェスが縮む。
fn vertical_step(env: &Env<'_>, log: &mut Log) {
    let (start, before) = (env.shared.workspace.get(), env.web_w());
    let want = before + DRAG_WEB;
    let s = env.scale();
    let from = vertical_grip(env);
    let to = (from.0 - (DRAG_WEB * s) as i32, from.1);
    // つかんでいる間（仕切りがアクセントの色）を撮る。サーフェスが縮むと Windows が本物のカーソルの位置で
    // `WM_MOUSEMOVE` を合成するので、つかんだままの位置は本物のカーソルの位置に動く。撮ったら Esc で取り消す。
    surface::post_press(env.surface_hwnd, from, to);
    let grabbed = wait_until(WAIT, || env.web_w() > before + TOL);
    let hot = splitter::hot() == Some(mm_ui_layout::Splitter::Vertical);
    env.shoot(log, "2-drag-vertical-held");
    surface::post_escape(env.surface_hwnd);
    let cancelled = wait_until(WAIT, || {
        env.shared.workspace.get() == start && close(env.web_w(), before)
    });
    surface::post_release(env.surface_hwnd, to);
    // 押す、動かす、離すを続けて投げる。合成された `WM_MOUSEMOVE` は離した後に届き、ホバーになるだけ。
    let from = vertical_grip(env);
    let to = (from.0 - (DRAG_WEB * s) as i32, from.1);
    surface::post_drag(env.surface_hwnd, from, to);
    let moved = grabbed && wait_until(WAIT, || close(env.web_w(), want));
    let right = Workspace::default().columns(env.window_w()).right;
    let (dom_ok, dom) = columns_match(env, want - right, right);
    let surface_px = env.shared.surface_size().0;
    let want_px = (env.window_w() * s).round() as i64 - (want * s).round() as i64;
    let surface_ok = (i64::from(surface_px) - want_px).abs() <= 1;
    env.shoot(log, "3-dragged-vertical");
    log.step(
        "drag_vertical",
        moved && hot && cancelled && dom_ok && surface_ok,
        json!({ "before": before, "want": want, "moved": moved, "hot_while_held": hot,
                "cancelled_by_escape": cancelled, "dom": dom, "surface_px": surface_px,
                "want_surface_px": want_px }),
    );
}

/// 横の仕切りをつかんで上へ動かし、Timeline が高くなる。
fn horizontal_step(env: &Env<'_>, log: &mut Log) {
    let (w, h) = env.surface();
    let ratio = env.shared.workspace.get().timeline_ratio;
    let gap = splitter_rect(content_width(w, true), h, ratio);
    let s = env.scale() as f32;
    let from = (
        (w / 4.0 * s) as i32,
        ((gap.y + gap.h / 2.0) * s).round() as i32,
    );
    let to = (from.0, from.1 - (DRAG_TIMELINE * s) as i32);
    let want = (timeline_height(h, ratio) + DRAG_TIMELINE) / h;
    surface::post_drag(env.surface_hwnd, from, to);
    let moved = wait_until(WAIT, || {
        (env.shared.workspace.get().timeline_ratio - want).abs() <= RATIO_TOL
    });
    env.shoot(log, "4-dragged-horizontal");
    log.step(
        "drag_horizontal",
        moved,
        json!({ "before": ratio, "want": want, "after": env.shared.workspace.get().timeline_ratio }),
    );
}

/// 割り付けを設定のファイルに書き、Reset UI Layout で既定に戻し、書いた設定を読み直して同じ割り付けに戻る。
fn persist_step(env: &Env<'_>, log: &mut Log) {
    let dragged = env.shared.workspace.get();
    let path = env.test.report.with_extension("workspace.json");
    let wrote = workspace::write(&path, &dragged).is_ok();
    let reset = commands::run(env.shared, "view.reset_ui_layout").is_ok()
        && wait_until(WAIT, || env.shared.workspace.get() == Workspace::default());
    let (reset_dom, _) = columns_match(env, 260.0, 400.0);
    env.shoot(log, "5-reset-ui-layout");
    // 再起動の代わりに、アプリが起動で設定を読んで置くのと同じ経路（読む、置く、配置を直す）を通す。
    let saved = workspace::read(&path);
    if let Some(saved) = saved {
        env.shared.workspace.set(saved);
        layout::refresh();
    }
    let right = Workspace::default().columns(env.window_w()).right;
    let restored = saved == Some(dragged)
        && wait_until(WAIT, || {
            env.shared.workspace.get() == dragged && close(env.web_w(), dragged_web(&dragged, env))
        });
    let (dom_ok, dom) = columns_match(env, env.web_w() - right, right);
    env.shoot(log, "6-reopened");
    log.step(
        "persist_and_reset",
        wrote && reset && reset_dom && restored && dom_ok,
        json!({ "wrote": wrote, "path": path.display().to_string(), "reset": reset,
                "reset_dom": reset_dom, "restored": restored, "dom": dom }),
    );
}

fn dragged_web(ws: &Workspace, env: &Env<'_>) -> f64 {
    ws.columns(env.window_w()).web()
}

/// コマンドのチェックの状態（View のメニューの項目に出すもの）。
fn checked(shared: &Shared, id: &str) -> Option<bool> {
    commands::table(shared)
        .into_iter()
        .find(|c| c.id == id)?
        .state
        .checked
}

/// 左の列、右の列を順に閉じ、閉じた分だけサーフェスが広がり View のチェックが外れるのと、閉じた左の列のパネルを選ぶ
/// コマンド（View > Media）がその列を開けるのと、右の列をもう一度開けると戻るのを確かめる。
fn columns_step(env: &Env<'_>, log: &mut Log) {
    let run = |id: &str| commands::run(env.shared, id).is_ok();
    let surface_w = || f64::from(env.shared.surface_size().0) / env.scale();
    let open_w = surface_w();
    let left_closed = run("view.toggle_left_column");
    let grew = wait_until(WAIT, || surface_w() > open_w + 200.0);
    let unchecked = checked(env.shared, "view.toggle_left_column") == Some(false);
    let (dom_left, dom_l) = columns_match(env, 0.0, 400.0);
    env.shoot(log, "7-left-closed");
    let revealed = run("view.show_media") && env.shared.workspace.get().left_open;
    let right_closed = run("view.toggle_right_column");
    let (dom_right, dom_r) = columns_match(env, 260.0, 0.0);
    env.shoot(log, "8-right-closed");
    run("view.toggle_right_column");
    let both_back = wait_until(WAIT, || (surface_w() - open_w).abs() <= TOL);
    let ok = left_closed && grew && unchecked && dom_left && revealed && right_closed;
    log.step(
        "close_columns",
        ok && dom_right && both_back,
        json!({ "surface_open": open_w, "unchecked_when_closed": unchecked,
                "opened_by_panel_command": revealed, "dom_left_closed": dom_l,
                "dom_right_closed": dom_r }),
    );
}

/// 左の列を閉じたまま WebView を読み直しても、閉じたままか（ページを読み終えたら列の幅と開閉を送り直す）。
fn reload_step(env: &Env<'_>, log: &mut Log) {
    let closed = commands::run(env.shared, "view.toggle_left_column").is_ok();
    let (closed_dom, _) = columns_match(env, 0.0, 400.0);
    // 読み直す前のページで列が合って見えないよう、フラグを立てたページが新しいページに替わるのを待つ。
    eval_json(env.webview, "window.__mmBeforeReload = true");
    let fresh_page = "window.__mmBeforeReload !== true && document.readyState === 'complete'";
    let reloaded = env.webview.reload().is_ok()
        && wait_until(WAIT, || {
            eval_json(env.webview, fresh_page) == Value::Bool(true)
        });
    let (dom_ok, dom) = columns_match(env, 0.0, 400.0);
    env.shoot(log, "9-reloaded-left-closed");
    commands::run(env.shared, "view.toggle_left_column").ok();
    log.step(
        "reload_keeps_columns",
        closed && closed_dom && reloaded && dom_ok,
        json!({ "reloaded": reloaded, "dom": dom }),
    );
}

/// 割り付けの工程。終わったら既定の割り付けに戻す（後の工程と、次の実行に影響しない）。
pub fn run(
    shared: &Arc<Shared>,
    window: &Window,
    webview: Option<Webview>,
    surface_hwnd: isize,
    window_hwnd: isize,
    test: &PlayTest,
) -> Value {
    let Some(webview) = webview.as_ref() else {
        return json!({ "skipped": "WebView が無い" });
    };
    let env = &Env {
        shared,
        window,
        webview,
        surface_hwnd,
        window_hwnd,
        test,
    };
    let mut log = Log::default();
    let (default_ok, dom) = columns_match(env, 260.0, 400.0);
    log.step("default_layout", default_ok, json!({ "dom": dom }));
    env.shoot(&mut log, "1-default");
    vertical_step(env, &mut log);
    horizontal_step(env, &mut log);
    persist_step(env, &mut log);
    columns_step(env, &mut log);
    reload_step(env, &mut log);
    shared.workspace.set(Workspace::default());
    layout::refresh();
    log.into_json()
}
