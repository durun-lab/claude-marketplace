//! `--play-test` の Timeline のツールバーの通し。実際のウィンドウへのクリックで、Snapping（入/切の往復）、Zoom In（倍率が √2 倍）、
//! Full Extent Zoom（全体表示と、もう一度押して戻る）、Split Clip と Undo（カットの数が 1 つ増えて戻る）を押し、
//! 表示の状態とプロジェクトのデータが期待どおり変わったかを確かめる。ボタンの位置は `Ui` と同じ `mm-ui-layout` の関数で引く。

use std::sync::Arc;
use std::time::Duration;

use mm_ui_layout::{TimeView, TimelineModel, VIEW_MARGIN};
use mm_ui_wgpu::timeline_model;
use serde_json::{json, Map, Value};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::playback::Command;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_edit::{cut_span_secs, cuts};
use crate::win::verify_place::{placement, Placement};
use crate::win::verify_stage::{checkpoint, wait_until};

/// 入力を投げてから結果を見るまでの待ち。
const SETTLE: Duration = Duration::from_millis(300);
/// 結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// 倍率を比べるときの誤差（相対）。
const EPS: f64 = 1e-6;

/// 試験の対象。
pub struct Env<'a> {
    pub shared: &'a Arc<Shared>,
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

fn model(shared: &Shared) -> Option<TimelineModel> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    Some(timeline_model(&doc.project, comp))
}

fn place_of(env: &Env<'_>) -> Option<Placement> {
    placement(env.shared, env.surface_hwnd, &model(env.shared)?)
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
fn shot(env: &Env<'_>, log: &mut Log, name: &str) {
    let Some(base) = &env.test.screenshot else {
        return;
    };
    std::thread::sleep(SETTLE);
    let path = base.with_extension(format!("toolbar-{name}.png"));
    match capture::capture_window(env.window_hwnd, &path) {
        Ok(()) => log.shots.push(path.display().to_string()),
        Err(e) => log.shots.push(format!("失敗: {e}")),
    }
    checkpoint();
}

/// ツールバーのボタンを押して離す。ツールバーかボタンが無ければ false。
fn press(env: &Env<'_>, command: &str) -> bool {
    let Some((x, y)) = place_of(env).and_then(|p| p.toolbar_button(command)) else {
        return false;
    };
    surface::post_click(env.surface_hwnd, x, y);
    true
}

/// 描画スレッドが置いた今の表示。
fn view(shared: &Shared) -> Option<TimeView> {
    shared.view.current().map(|v| v.time)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS * a.abs().max(b.abs()).max(1.0)
}

/// Snapping を押して、入にし（入の見た目のスクリーンショットを撮る）、切にし、入に戻す（既定は入）。
fn snapping_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let first = shared.view.snapping();
    let toggle = |want: bool| {
        press(env, "timeline.toggle_snapping")
            && wait_until(WAIT, || shared.view.snapping() == want)
    };
    let on = first || toggle(true);
    shot(env, log, "1-snapping-on");
    let off = toggle(false);
    shot(env, log, "2-snapping-off");
    let back_on = toggle(true);
    log.step(
        "snapping",
        on && off && back_on,
        json!({ "initial": first, "on": on, "off": off, "back_on": back_on }),
    );
}

/// Zoom In を押して、倍率が √2 倍になるかを確かめる（上限なら押しても変わらないので確かめない）。
fn zoom_in_step(env: &Env<'_>, log: &mut Log) {
    let Some(before) = view(env.shared) else {
        log.step("zoom_in", false, json!("表示の状態が無い"));
        return;
    };
    let want = (before.px_per_sec * std::f64::consts::SQRT_2).min(mm_ui_layout::MAX_PX_PER_SEC);
    let ok = press(env, "timeline.zoom_in")
        && wait_until(WAIT, || {
            view(env.shared).is_some_and(|v| close(v.px_per_sec, want))
        });
    log.step(
        "zoom_in",
        ok,
        json!({ "before": before.px_per_sec, "want": want, "after": view(env.shared).map(|v| v.px_per_sec) }),
    );
    shot(env, log, "3-zoom-in");
}

/// Full Extent Zoom を押して全体表示にし、もう一度押して戻る。
fn full_extent_step(env: &Env<'_>, log: &mut Log, duration: f64) {
    let shared = env.shared;
    let Some(width) = place_of(env).map(|p| p.viewport().layout.row_area.w) else {
        log.step("full_extent", false, json!("割り付けが無い"));
        return;
    };
    let fit = TimeView::fit(duration, width, VIEW_MARGIN);
    let before = view(shared);
    let shown =
        press(env, "timeline.full_extent_zoom") && wait_until(WAIT, || view(shared) == Some(fit));
    shot(env, log, "4-full-extent");
    let back =
        press(env, "timeline.full_extent_zoom") && wait_until(WAIT, || view(shared) == before);
    log.step(
        "full_extent",
        shown && back,
        json!({ "fit": fit.px_per_sec, "shown": shown, "returned": back }),
    );
}

/// 再生位置を先頭のカットの中央へ動かして Split Clip を押し、カットが 1 つ増えるのを確かめ、Undo で戻す。
fn split_undo_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let Some(first) = cuts(shared).first().map(|c| c.id.to_string()) else {
        log.skip("split_and_undo", "シーケンスのカットが無い");
        return;
    };
    let Some((start, end)) = cut_span_secs(shared, &first) else {
        log.skip("split_and_undo", "カットの位置が無い");
        return;
    };
    let before = cuts(shared).len();
    let at = (start + end) / 2.0;
    shared.send(Command::Seek(at));
    wait_until(WAIT, || (shared.playhead() - at).abs() < 0.05);
    let split = press(env, "clip.split") && wait_until(WAIT, || cuts(shared).len() == before + 1);
    shot(env, log, "5-split");
    // 適用されなかった分割の取り消しは、1 つ前のトランザクションを戻してしまうので押さない。
    let undone =
        split && press(env, "edit.undo") && wait_until(WAIT, || cuts(shared).len() == before);
    log.step(
        "split_and_undo",
        split && undone,
        json!({ "cuts_before": before, "split": split, "undone": undone }),
    );
}

/// ツールバーの工程。
pub fn run(env: &Env<'_>) -> Value {
    let mut log = Log::default();
    let duration = model(env.shared).map_or(0.0, |m| m.duration);
    let has_bar = place_of(env).is_some_and(|p| p.toolbar_button("edit.undo").is_some());
    log.step("toolbar_visible", has_bar, json!({}));
    if has_bar {
        snapping_step(env, &mut log);
        zoom_in_step(env, &mut log);
        full_extent_step(env, &mut log, duration);
        split_undo_step(env, &mut log);
    }
    log.into_json()
}
