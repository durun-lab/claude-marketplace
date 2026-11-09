//! `--play-test` の Timeline 編集の通し。実際のウィンドウへのキーとドラッグで、Source を開いてイン点・アウト点を付け、
//! insert、端のドラッグ（ripple）、中のドラッグ（slip）、split、lift、取り消し、レイヤーの端のドラッグ（Esc での取り消しと trimLayer）、
//! スナップ（再生位置とカットの端、N を押している間の反転、再生位置の線の手前のツールチップ）を順に行い、そのたびに
//! プロジェクトのデータが期待どおり変わったかを確かめて、段ごとにスクリーンショットを残す。

use std::sync::Arc;
use std::time::Duration;

use mm_doc::{Cut, Project, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_types::{Id, ParamValue};
use mm_ui_wgpu::timeline_model;
use serde_json::{json, Value};

use crate::win::capture;
use crate::win::cli::PlayTest;
use crate::win::keys;
use crate::win::playback::Command;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_place::{placement, Placement};
use crate::win::verify_stage::{checkpoint, wait_until};

/// 仮想キーコード。
const VK_I: u32 = 0x49;
const VK_O: u32 = 0x4F;
const VK_B: u32 = 0x42;
const VK_COMMA: u32 = 0xBC;
const VK_DELETE: u32 = 0x2E;
const VK_N: u32 = 0x4E;
/// 入力を投げてから結果を見るまでの待ち。
const SETTLE: Duration = Duration::from_millis(300);
/// 結果が出るのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// Source に付けるイン点・アウト点（秒）。
const MARK_IN: f64 = 0.5;
const MARK_OUT: f64 = 1.5;
/// insert する再生位置（先頭のカットの後半）。
const INSERT_AT: f64 = 0.8;
/// 端と中のドラッグの量（秒）。
const DRAG_SECS: f64 = 0.25;
/// レイヤーのアウト点のつまみを引く量（秒）。帯が縮んだのが画面で分かる長さにする。
const TRIM_SECS: f64 = 1.5;
/// 端のつまみの中心を掴むための x の内側へのずれ（論理ピクセル）。
const HANDLE_INSET: f32 = 3.0;
/// スナップの目標の右へドラッグする量（論理ピクセル）。吸い付く距離（8）の内側にし、吸い付かなければ端がずれて分かるようにする。
const SNAP_NEAR: f32 = 4.0;
/// 再生位置の左へドラッグする量（論理ピクセル）。ポインターの右 16 から始まるツールチップの矩形の中を、再生位置の線が通る。
const TOOLTIP_LEFT: f32 = 40.0;

/// 試験の対象。
pub struct Env<'a> {
    pub shared: &'a Arc<Shared>,
    pub surface_hwnd: isize,
    pub window_hwnd: isize,
    pub test: &'a PlayTest,
}

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

fn sequence_of(project: &Project) -> Option<&Sequence> {
    let comp = project.compositions.first()?;
    let layer = comp
        .layers
        .iter()
        .find(|l| l.source.kind == SEQUENCE_KIND)?;
    project
        .sequence(&Id::new(layer.source.text_param(SEQUENCE_PARAM)?))
        .ok()
}

fn cuts(shared: &Shared) -> Vec<Cut> {
    shared
        .doc()
        .and_then(|d| sequence_of(&d.project).map(|s| s.cuts.clone()))
        .unwrap_or_default()
}

fn cut_named(shared: &Shared, id: &str) -> Option<Cut> {
    cuts(shared).into_iter().find(|c| c.id.as_str() == id)
}

/// スクリーンショットを撮る。返るまで長くかかったら、時間切れの工程はここで止まり、後の入力を送らない。
fn shot(env: &Env<'_>, log: &mut Log, name: &str) {
    let Some(base) = &env.test.screenshot else {
        return;
    };
    std::thread::sleep(SETTLE);
    let path = base.with_extension(format!("edit-{name}.png"));
    match capture::capture_window(env.window_hwnd, &path) {
        Ok(()) => log.shots.push(path.display().to_string()),
        Err(e) => log.shots.push(format!("失敗: {e}")),
    }
    checkpoint();
}

/// 画面の位置の計算に要るもの。割り付けと Timeline の見えている所、見えているシーケンスの行の添字。
struct Geometry {
    place: Placement,
    row: usize,
}

fn geometry(shared: &Shared, hwnd: isize) -> Option<Geometry> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let model = timeline_model(&doc.project, comp);
    let place = placement(shared, hwnd, &model)?;
    let row = place.first_visible(|i| !model.items[i].cuts.is_empty())?;
    Some(Geometry { place, row })
}

impl Geometry {
    /// 行 `row` の帯の縦の中央で、時刻 `t` から論理ピクセルで `dx` ずらした点（クライアントの物理ピクセル）。
    fn at(&self, row: usize, t: f64, dx: f32) -> (i32, i32) {
        self.place.bar_point(row, t, dx)
    }
}

/// カット `id` の Timeline の上の範囲（秒）。
fn cut_span_secs(shared: &Shared, id: &str) -> Option<(f64, f64)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let ids = mm_ui_wgpu::timeline_cut_ids(&doc.project, comp);
    let model = timeline_model(&doc.project, comp);
    ids.iter().enumerate().find_map(|(item, list)| {
        let k = list.iter().position(|c| c == id)?;
        let bar = model.items.get(item)?.cuts.get(k)?;
        Some((bar.start, bar.end))
    })
}

fn seek_main(shared: &Shared, t: f64) -> bool {
    shared.send(Command::Seek(t));
    wait_until(WAIT, || (shared.playhead() - t).abs() < 0.05)
}

/// Source を開いてイン点・アウト点を付ける。
fn mark_source(env: &Env<'_>, log: &mut Log) -> Option<String> {
    let shared = env.shared;
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let asset = doc.project.assets.last()?.id.to_string();
    let opened = shared.source.open(&doc.project, comp, &asset);
    let source = shared.source.handle();
    // 後の割り付けが Source の領域を含むよう、フレームの到着に加えて `Ui` が領域を出すまで待つ。
    let framed = source.as_ref().is_some_and(|s| {
        wait_until(WAIT, || {
            s.shared.frames.newer_than(0).is_some() && shared.source.is_shown()
        })
    });
    log.step(
        "source_open",
        opened.is_ok() && framed,
        json!({ "asset": asset }),
    );
    let source = source?;
    for (t, vk) in [(MARK_IN, VK_I), (MARK_OUT, VK_O)] {
        source.shared.send(Command::Seek(t));
        wait_until(WAIT, || (source.shared.playhead() - t).abs() < 0.05);
        surface::post_key(env.surface_hwnd, vk);
        std::thread::sleep(SETTLE);
    }
    let marked =
        wait_until(WAIT, || {
            let d = shared.doc();
            d.is_some_and(|d| {
                d.project.bins.marks.iter().any(|m| {
                    m.in_point.is_some() && m.out_point.is_some() && m.asset.as_str() == asset
                })
            })
        });
    log.step(
        "source_marks",
        marked,
        json!({ "in": MARK_IN, "out": MARK_OUT }),
    );
    shot(env, log, "1-source");
    Some(asset)
}

/// insert のキーで、Source の範囲が再生位置のカットの編集点に入る。入ったカットの ID を返す。
fn insert_step(env: &Env<'_>, log: &mut Log, asset: &str) -> Option<String> {
    let shared = env.shared;
    let before = cuts(shared).len();
    seek_main(shared, INSERT_AT);
    surface::post_key(env.surface_hwnd, VK_COMMA);
    let grew = wait_until(WAIT, || cuts(shared).len() == before + 1);
    let id = shared.edit.selected_cut();
    let cut = id.as_deref().and_then(|i| cut_named(shared, i));
    let ranged = cut.as_ref().is_some_and(|c| {
        (c.in_point.to_f64() - MARK_IN).abs() < 1e-6
            && (c.out_point.to_f64() - MARK_OUT).abs() < 1e-6
            && c.source
                .params
                .get("asset")
                .is_some_and(|p| matches!(&p.value, ParamValue::AssetRef(a) if a.as_str() == asset))
    });
    log.step(
        "insert",
        grew && ranged,
        json!({ "cuts_before": before, "cuts_after": cuts(shared).len(), "cut": id }),
    );
    shot(env, log, "2-insert");
    id
}

/// 挿入したカットの出つまみを引いて ripple、中を引いて slip。
fn drag_steps(env: &Env<'_>, log: &mut Log, id: &str) {
    let shared = env.shared;
    let (Some(g), Some((start, end))) = (
        geometry(shared, env.surface_hwnd),
        cut_span_secs(shared, id),
    ) else {
        log.step("drag", false, json!("Timeline の位置を引けない"));
        return;
    };
    let out_before = cut_named(shared, id).map(|c| c.out_point.to_f64());
    let from = g.at(g.row, end, -HANDLE_INSET);
    let to = g.at(g.row, end - DRAG_SECS, -HANDLE_INSET);
    surface::post_drag(env.surface_hwnd, from, to);
    let rippled = wait_until(WAIT, || {
        cut_named(shared, id).map(|c| c.out_point.to_f64()) != out_before
    });
    let out_after = cut_named(shared, id).map(|c| c.out_point.to_f64());
    log.step(
        "ripple_drag",
        rippled,
        json!({ "out_before": out_before, "out_after": out_after }),
    );
    shot(env, log, "3-ripple");

    let in_before = cut_named(shared, id).map(|c| c.in_point.to_f64());
    let mid = (start + end - DRAG_SECS) / 2.0;
    let by = (DRAG_SECS * g.place.px_per_sec() / 2.0) as f32;
    surface::post_drag(
        env.surface_hwnd,
        g.at(g.row, mid, 0.0),
        g.at(g.row, mid, by),
    );
    let slipped = wait_until(WAIT, || {
        cut_named(shared, id).map(|c| c.in_point.to_f64()) != in_before
    });
    let in_after = cut_named(shared, id).map(|c| c.in_point.to_f64());
    log.step(
        "slip_drag",
        slipped,
        json!({ "in_before": in_before, "in_after": in_after }),
    );
    shot(env, log, "4-slip");
}

/// 再生位置で split、選んだカットの lift、取り消し。
fn cut_steps(env: &Env<'_>, log: &mut Log, id: &str) {
    let shared = env.shared;
    let Some((start, end)) = cut_span_secs(shared, id) else {
        log.step("split", false, json!("カットの位置が無い"));
        return;
    };
    let before = cuts(shared).len();
    seek_main(shared, (start + end) / 2.0);
    surface::post_key(env.surface_hwnd, VK_B);
    let split = wait_until(WAIT, || cuts(shared).len() == before + 1);
    log.step(
        "split",
        split,
        json!({ "cuts_before": before, "cuts_after": cuts(shared).len() }),
    );
    shot(env, log, "5-split");

    let before_gaps = cuts(shared).iter().filter(|c| c.source.is_gap()).count();
    surface::post_key(env.surface_hwnd, VK_DELETE);
    let lifted = wait_until(WAIT, || {
        cuts(shared).iter().filter(|c| c.source.is_gap()).count() == before_gaps + 1
    });
    log.step("lift", lifted, json!({ "gaps_before": before_gaps }));
    shot(env, log, "6-lift");

    // 適用されなかった編集の取り消しは、1 つ前のトランザクション（テストの前のユーザーの操作のこともある）を戻してしまう。
    if !lifted {
        log.step(
            "undo_lift",
            false,
            json!("lift が適用されなかったので取り消さない"),
        );
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || {
        cuts(shared).iter().filter(|c| c.source.is_gap()).count() == before_gaps
    });
    log.step("undo_lift", undone, json!({}));
    shot(env, log, "7-undo");
}

/// 行の領域に見えている、シーケンスでない最初のレイヤーの、Timeline の行の添字、ID、終わり（秒）。
fn trim_target(shared: &Shared, place: &Placement) -> Option<(usize, String, f64)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let ids = mm_ui_wgpu::timeline_layer_ids(comp);
    let model = timeline_model(&doc.project, comp);
    let row = place.first_visible(|i| model.items[i].cuts.is_empty())?;
    Some((row, ids.get(row)?.clone(), model.items[row].end))
}

fn layer_out(shared: &Shared, id: &str) -> Option<f64> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.out_point.to_f64())
}

/// レイヤーのアウト点のつまみを引くドラッグ。レイヤーの ID、掴む所、引いた先（サーフェスの座標）。
struct TrimDrag {
    id: String,
    from: (i32, i32),
    to: (i32, i32),
}

fn trim_drag(shared: &Shared, hwnd: isize) -> Option<TrimDrag> {
    let g = geometry(shared, hwnd)?;
    let (row, id, end) = trim_target(shared, &g.place)?;
    Some(TrimDrag {
        id,
        from: g.at(row, end, -HANDLE_INSET),
        to: g.at(row, end - TRIM_SECS, -HANDLE_INSET),
    })
}

/// アウト点のつまみを引いたまま Esc を押して離し、データが変わらないことを確かめる。
fn trim_cancel_step(env: &Env<'_>, log: &mut Log, drag: &TrimDrag) {
    let before = layer_out(env.shared, &drag.id);
    surface::post_press(env.surface_hwnd, drag.from, drag.to);
    std::thread::sleep(SETTLE);
    surface::post_escape(env.surface_hwnd);
    surface::post_release(env.surface_hwnd, drag.to);
    std::thread::sleep(SETTLE);
    let kept = layer_out(env.shared, &drag.id) == before;
    log.step("trim_cancel", kept, json!({ "out_before": before }));
}

/// レイヤーのアウト点のつまみを引いて、離す前（帯だけが動く）と離した後（trimLayer）を撮り、取り消し 1 回で戻す。
fn trim_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let Some(drag) = trim_drag(shared, env.surface_hwnd) else {
        log.step("trim_drag", false, json!("レイヤーの位置を引けない"));
        return;
    };
    trim_cancel_step(env, log, &drag);
    let TrimDrag { id, from, to } = drag;
    let before = layer_out(shared, &id);
    surface::post_press(env.surface_hwnd, from, to);
    std::thread::sleep(SETTLE);
    let held = layer_out(shared, &id) == before;
    shot(env, log, "8-trim-dragging");
    surface::post_release(env.surface_hwnd, to);
    let trimmed = wait_until(WAIT, || layer_out(shared, &id) != before);
    let after = layer_out(shared, &id);
    log.step(
        "trim_drag",
        held && trimmed,
        json!({ "layer": id, "out_before": before, "out_after": after, "unchanged_while_dragging": held }),
    );
    shot(env, log, "9-trim");
    if !trimmed {
        log.step(
            "undo_trim",
            false,
            json!("trimLayer が適用されなかったので取り消さない"),
        );
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || layer_out(shared, &id) == before);
    log.step("undo_trim", undone, json!({}));
}

/// レイヤー `id` の、コンポジションの時間軸での終わり（秒）。
fn layer_end(shared: &Shared, id: &str) -> Option<f64> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.range().ok()?.end.to_f64())
}

/// 隙間でないカットのうち、いちばん後ろの終わり（秒）。
fn last_cut_end(shared: &Shared) -> Option<f64> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let model = timeline_model(&doc.project, comp);
    let cuts = model.items.iter().flat_map(|it| it.cuts.iter());
    cuts.filter(|c| !c.gap).map(|c| c.end).reduce(f64::max)
}

/// ドラッグを Esc で取り消して離す。
fn cancel_drag(hwnd: isize, at: (i32, i32)) {
    surface::post_escape(hwnd);
    surface::post_release(hwnd, at);
    std::thread::sleep(SETTLE);
}

/// レイヤーのアウト点のつまみを再生位置の近くへドラッグしたまま撮り（スナップの表示とドラッグ中のツールチップ）、N を押している間は
/// 吸い付きが外れることも撮る。再生位置の線がツールチップの中を通る所でも撮り、続けてカットの端の近くへドラッグして撮り、
/// 離してアウト点がその端に吸い付いたかを確かめる。
fn snap_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| {
        let target = trim_target(shared, &g.place)?;
        Some((g, target, last_cut_end(shared)?))
    });
    let Some((g, (row, id, end), edge)) = found else {
        log.step("snap_drag", false, json!("Timeline の位置を引けない"));
        return;
    };
    let from = g.at(row, end, -HANDLE_INSET);
    seek_main(shared, end - TRIM_SECS);
    surface::post_press(hwnd, from, g.at(row, shared.playhead(), SNAP_NEAR));
    shot(env, log, "10-snap-playhead");
    surface::post_key(hwnd, VK_N);
    shot(env, log, "11-snap-held-n");
    surface::post_key_up(hwnd, VK_N);
    cancel_drag(hwnd, from);
    let left = g.at(row, shared.playhead(), -TOOLTIP_LEFT);
    surface::post_press(hwnd, from, left);
    shot(env, log, "13-tooltip-over-playhead");
    cancel_drag(hwnd, from);
    let to = g.at(row, edge, SNAP_NEAR);
    surface::post_press(hwnd, from, to);
    shot(env, log, "12-snap-clip-edge");
    surface::post_release(hwnd, to);
    let moved = wait_until(WAIT, || layer_end(shared, &id) != Some(end));
    let after = layer_end(shared, &id);
    let snapped = after.is_some_and(|e| (e - edge).abs() < 1e-6);
    let kept = shared.view.snapping();
    log.step(
        "snap_drag",
        moved && snapped && kept,
        json!({ "layer": id, "edge": edge, "end_after": after, "snapping_kept": kept }),
    );
    if moved {
        keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
        let undone = wait_until(WAIT, || layer_end(shared, &id) == Some(end));
        log.step("undo_snap", undone, json!({}));
    }
}

/// Timeline 編集の通しを行って結果を返す。シーケンスのレイヤーが無いプロジェクトではスキップする。
pub fn run(env: &Env<'_>) -> Value {
    let mut log = Log::default();
    let has_sequence = env
        .shared
        .doc()
        .is_some_and(|d| sequence_of(&d.project).is_some());
    if !has_sequence {
        return json!({ "skipped": "シーケンスのレイヤーが無い" });
    }
    if let Some(asset) = mark_source(env, &mut log) {
        if let Some(id) = insert_step(env, &mut log, &asset) {
            drag_steps(env, &mut log, &id);
            cut_steps(env, &mut log, &id);
        }
    }
    trim_step(env, &mut log);
    snap_step(env, &mut log);
    log.into_json()
}
