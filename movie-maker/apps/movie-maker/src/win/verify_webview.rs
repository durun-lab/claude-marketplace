//! `--play-test` の WebView の工程。WebView の中でイベントを投げ、画面の様子を `eval_with_callback` で読んで確かめる。
//! WebView にフォーカスを移さずに確かめられるよう、イベントは WebView の中で作る。
//! - 橋（`web/transport.js`）: WebView で押した Space が再生を始めて止め、シークの頼みが再生位置を動かす。
//! - 割り付け: 中央の列が無く、WebView の幅が 2 列の幅の和。View > Diagnostics で開いたタブに GPU が出る。
//! - キーフレームの欄: キーフレームのあるレイヤーを選ぶと、Inspector のキーフレームの欄が行の右（3 列目）に出る。
//! - 編集の結果の文言: 失敗した編集の理由が、Shell の右の列の上に出る。

use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::Webview;

use crate::win::cli::PlayTest;
use crate::win::commands;
use crate::win::playback::Command;
use crate::win::state::Shared;
use crate::win::theme;
use crate::win::verify::shoot;
use crate::win::verify_stage::wait_until;

/// 投げたイベントの結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(3);
/// 結果が出てから WebView が描き終えるのを待つ。スクリーンショットに前の画が写らないように。
const PAINT_WAIT: Duration = Duration::from_millis(300);
/// シークの頼みで狙う時刻（秒）。`SEEK_JS` の `time` と同じ。
const SEEK_TARGET: f64 = 1.0;
/// シークの許容誤差（秒）。フレームへの丸めの分。
const SEEK_TOLERANCE: f64 = 0.1;
/// WebView の幅の許容誤差（CSS ピクセル）。物理ピクセルへの丸めの分。
const WIDTH_TOLERANCE: f64 = 1.0;
/// WebView の body に Space の keydown を投げる。WebView の部品にフォーカスが無いときにキーが届く所。
const SPACE_JS: &str = r#"document.body.dispatchEvent(new KeyboardEvent("keydown", { code: "Space", key: " ", bubbles: true, cancelable: true }))"#;
/// Inspector の前後のキーフレームへの移動と同じシークの頼み（`mm:seek`）を投げる。
const SEEK_JS: &str =
    r#"window.dispatchEvent(new CustomEvent("mm:seek", { detail: { time: 1 } }))"#;
/// 列の様子。中央の列の有無、WebView と右の列の幅（CSS ピクセル）、右の列のタブ、選んだタブ、Diagnostics の GPU の欄。
const COLUMNS_JS: &str = r#"(() => {
  const right = document.querySelector(".mm-shell-right");
  const tabs = [...document.querySelectorAll(".mm-shell-right-tabs [role=tab]")];
  const gpu = document.querySelector('.mm-shell-right .mm-diag dl[aria-label="Summary"] dd');
  return {
    center_column: document.querySelector(".mm-shell-center") !== null,
    webview_width: window.innerWidth,
    right_column_width: right ? right.getBoundingClientRect().width : null,
    right_tabs: tabs.map((t) => t.textContent),
    selected_tab: tabs.find((t) => t.getAttribute("aria-selected") === "true")?.textContent ?? null,
    gpu: gpu ? gpu.textContent : null,
  };
})()"#;
/// 右の列の先頭のタブ（Inspector）を選ぶ。
const INSPECTOR_TAB_JS: &str =
    r#"document.querySelector(".mm-shell-right-tabs [role=tab]")?.click()"#;
/// Inspector の幅と、キーフレームのある行のキーフレームの欄が置かれた列。
const STRIP_JS: &str = r#"(() => {
  const inspector = document.querySelector(".mm-shell-right .mm-insp");
  const strip = document.querySelector('.mm-shell-right .mm-insp-row > .mm-insp-strip[data-keyed="true"]');
  return {
    inspector_width: inspector ? inspector.getBoundingClientRect().width : null,
    strip_column: strip ? getComputedStyle(strip).gridColumnStart : null,
  };
})()"#;

/// Shell の右の列の上に出ている編集の結果の文言。
const MESSAGE_JS: &str =
    r#"document.querySelector(".mm-shell-right > [role=status]")?.textContent ?? null"#;

/// 工程に要るもの。
pub struct Env<'a> {
    pub shared: &'a Arc<Shared>,
    pub webview: Option<Webview>,
    pub window_hwnd: isize,
    pub test: &'a PlayTest,
}

/// 橋、割り付け、キーフレームの欄、編集の結果の文言を順に確かめる。
pub fn run(env: &Env) -> Value {
    let Some(webview) = env.webview.as_ref() else {
        return json!({ "skipped": "WebView が無い" });
    };
    let keys = keys(env.shared, webview);
    let columns = columns(env, webview);
    let strip = keyframe_strip(env, webview);
    let message = edit_message(env, webview);
    let ok = |v: &Value| v["ok"] == true || v.get("skipped").is_some();
    json!({
        "all_ok": ok(&keys) && ok(&columns) && ok(&strip) && ok(&message),
        "keys": keys,
        "columns": columns,
        "keyframe_strip": strip,
        "edit_message": message,
    })
}

/// WebView で `js` を評価して、結果の JSON を返す。`WAIT` までに返らなければ `Value::Null`。
fn eval_json(webview: &Webview, js: &str) -> Value {
    let (tx, rx) = channel();
    let sent = webview.eval_with_callback(js, move |text| {
        let _ = tx.send(text);
    });
    if sent.is_err() {
        return Value::Null;
    }
    rx.recv_timeout(WAIT)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

/// Space で再生が始まって止まり、シークの頼みで再生位置が動くか。先頭から再生し、短いコンポジションでも
/// 2 回目の Space より先に終わりに着かないようにする。
fn keys(shared: &Shared, webview: &Webview) -> Value {
    shared.send(Command::Seek(0.0));
    wait_until(WAIT, || shared.playhead().abs() <= SEEK_TOLERANCE);
    let playing = || shared.playing.load(Ordering::SeqCst);
    let before = playing();
    let _ = webview.eval(SPACE_JS);
    let started = !before && wait_until(WAIT, playing);
    let _ = webview.eval(SPACE_JS);
    let stopped = wait_until(WAIT, || !playing());
    let _ = webview.eval(SEEK_JS);
    let sought = wait_until(WAIT, || {
        (shared.playhead() - SEEK_TARGET).abs() <= SEEK_TOLERANCE
    });
    json!({
        "ok": started && stopped && sought,
        "space_starts_playback": started,
        "space_stops_playback": stopped,
        "seek_request_moves_playhead": sought,
    })
}

/// 中央の列が無く WebView の幅が 2 列の和か。View > Diagnostics（メニューと同じ `commands::run`）で開いたタブに
/// 描画スレッドが選んだ GPU が出るか。撮った後は Inspector のタブに戻す。
fn columns(env: &Env, webview: &Webview) -> Value {
    let before = eval_json(webview, COLUMNS_JS);
    let width_ok = before["webview_width"]
        .as_f64()
        .is_some_and(|w| (w - theme::webview_width()).abs() <= WIDTH_TOLERANCE);
    let opened = commands::run(env.shared, "view.show_diagnostics").is_ok();
    let adapter = env.shared.adapter.lock().expect("adapter").clone();
    let mut after = Value::Null;
    let shown = wait_until(WAIT, || {
        after = eval_json(webview, COLUMNS_JS);
        after["selected_tab"] == "Diagnostics" && after["gpu"] == adapter.as_str()
    });
    std::thread::sleep(PAINT_WAIT);
    let shot = shoot(env.window_hwnd, env.test, "diagnostics");
    let _ = webview.eval(INSPECTOR_TAB_JS);
    json!({
        "ok": before["center_column"] == false && width_ok && opened && shown,
        "center_column": before["center_column"],
        "webview_width": before["webview_width"],
        "right_column_width": before["right_column_width"],
        "right_tabs": before["right_tabs"],
        "diagnostics_from_view_menu": opened && shown,
        "diagnostics_gpu": after["gpu"],
        "screenshot": shot,
    })
}

/// キーフレームのあるレイヤーを選び、Inspector のキーフレームの欄が行の右（3 列目）に出るか。撮った後は選択を戻す。
fn keyframe_strip(env: &Env, webview: &Webview) -> Value {
    let Some(layer) = keyframed_layer(env.shared) else {
        return json!({ "skipped": "キーフレームのあるレイヤーが無い" });
    };
    let previous = env.shared.selected_layer();
    env.shared.select_layer(Some(layer.clone()));
    let mut state = Value::Null;
    let placed = wait_until(WAIT, || {
        state = eval_json(webview, STRIP_JS);
        state["strip_column"] == "3"
    });
    std::thread::sleep(PAINT_WAIT);
    let shot = shoot(env.window_hwnd, env.test, "inspector-keyframes");
    env.shared.select_layer(previous);
    json!({
        "ok": placed,
        "layer": layer,
        "inspector_width": state["inspector_width"],
        "strip_column": state["strip_column"],
        "screenshot": shot,
    })
}

/// Source に素材が無いときの Mark In（データを変えずに失敗する編集）の理由が、Shell の右の列の上に出るか。
/// 撮った後は文言を消す。
fn edit_message(env: &Env, webview: &Webview) -> Value {
    let shared = env.shared;
    if shared.source.handle().is_some() {
        return json!({ "skipped": "Source に素材が開いている" });
    }
    let _ = commands::run_from_key(shared, "source.mark_in");
    wait_until(WAIT, || !shared.edit.message().is_empty());
    let expected = shared.edit.message();
    let mut shown = Value::Null;
    let ok = wait_until(WAIT, || {
        shown = eval_json(webview, MESSAGE_JS);
        !expected.is_empty() && shown == expected.as_str()
    });
    std::thread::sleep(PAINT_WAIT);
    let shot = shoot(env.window_hwnd, env.test, "edit-message");
    shared.edit.notify(String::new());
    json!({ "ok": ok, "message": expected, "shown": shown, "screenshot": shot })
}

/// 開いているコンポジションの、キーフレームのある一番上のレイヤー。
fn keyframed_layer(shared: &Shared) -> Option<String> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    comp.layers
        .iter()
        .rev()
        .find(|l| !l.keyframe_times().is_empty())
        .map(|l| l.id.to_string())
}
