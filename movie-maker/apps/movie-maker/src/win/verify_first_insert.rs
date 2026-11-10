//! `--play-test` の、sequence のレイヤーが無いコンポジションへの最初の insert の通し（`mm new` の後に素材を取り込んだ形）。
//! 実際のウィンドウへのキーで Source にイン点とアウト点を付けて `,` を押し、sequence と sequence のレイヤーとカットが入ることと、
//! 取り消し 1 回で全部消えることを確かめ、工程ごとにスクリーンショットを残す。間に、入れたカットを Backspace で抜いて
//! 空の sequence を残し、取り消し、やり直し、空の sequence への insert を通す。

use mm_doc::{Project, SEQUENCE_KIND};
use serde_json::json;

use crate::win::keys;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_edit::{
    cuts, mark_source, sequence_of, shot, tools, Env, Log, MARK_IN, MARK_OUT, VK_COMMA, WAIT,
};
use crate::win::verify_stage::wait_until;

const VK_BACKSPACE: u32 = 0x08;

/// 秒の比較の許容差。
const EPS: f64 = 1e-6;

/// 最初のコンポジションのレイヤーの数と長さ（秒）。
fn comp_shape(project: &Project) -> Option<(usize, f64)> {
    let comp = project.compositions.first()?;
    Some((comp.layers.len(), comp.duration.to_f64()))
}

/// 最初のコンポジションの sequence のレイヤーの長さ（秒）。
fn layer_secs(project: &Project) -> Option<f64> {
    let comp = project.compositions.first()?;
    let layer = comp
        .layers
        .iter()
        .find(|l| l.source.kind == SEQUENCE_KIND)?;
    Some(layer.duration().ok()?.to_f64())
}

/// 今の sequence のレイヤーの長さ、コンポジションの長さ、コンポジションの 1 フレームの長さ（秒）。
fn lengths(shared: &Shared) -> [Option<f64>; 3] {
    let doc = shared.doc();
    let project = doc.as_ref().map(|d| d.project.as_ref());
    [
        project.and_then(layer_secs),
        project.and_then(comp_shape).map(|(_, s)| s),
        project
            .and_then(|p| p.compositions.first())
            .map(|c| 1.0 / c.fps.to_f64()),
    ]
}

/// `a` と `b` がどちらもあって、許容差の中で等しいか。
fn same(a: Option<f64>, b: Option<f64>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if (a - b).abs() < EPS)
}

/// 編集の後のコンポジションの長さが決まりどおりか。レイヤーが無かったら sequence のレイヤーの長さ `fitted`、ほかのレイヤーがあれば縮まない。
fn comp_fits(before: (usize, f64), after: Option<f64>, fitted: f64) -> bool {
    match (before, after) {
        ((0, _), Some(after)) => (after - fitted).abs() < EPS,
        ((_, secs), Some(after)) => after > secs - EPS,
        (_, None) => false,
    }
}

/// `,` で、Source の範囲のカット 1 つの sequence と、カットの長さの sequence のレイヤーができる。insert が入れば真。
fn insert_step(env: &Env<'_>, log: &mut Log, before: (usize, f64)) -> bool {
    let shared = env.shared;
    surface::post_key(env.surface_hwnd, VK_COMMA);
    let made = wait_until(WAIT, || cuts(shared).len() == 1);
    let cut = cuts(shared).into_iter().next();
    let ranged = cut.as_ref().is_some_and(|c| {
        (c.in_point.to_f64() - MARK_IN).abs() < EPS && (c.out_point.to_f64() - MARK_OUT).abs() < EPS
    });
    let length = MARK_OUT - MARK_IN;
    let [layer, comp, _] = lengths(shared);
    log.step(
        "first_insert",
        made && ranged && same(layer, Some(length)) && comp_fits(before, comp, length),
        json!({ "cut": cut.map(|c| c.id.to_string()), "layer_secs": layer, "comp_secs": comp }),
    );
    shot(env, log, "2-first-insert");
    made
}

/// 入れた唯一のカットを Backspace（extract）で抜くと、sequence は空で残り、sequence のレイヤーは 1 フレームに縮み、
/// 再生位置はコンポジションの中に寄る。抜けたら真。
fn extract_step(env: &Env<'_>, log: &mut Log, before: (usize, f64)) -> bool {
    let shared = env.shared;
    surface::post_key(env.surface_hwnd, VK_BACKSPACE);
    let emptied = wait_until(WAIT, || cuts(shared).is_empty());
    let sequence_kept = shared
        .doc()
        .is_some_and(|d| sequence_of(&d.project).is_some());
    let [layer, comp, frame] = lengths(shared);
    let inside = wait_until(WAIT, || comp.is_some_and(|c| shared.playhead() < c));
    let shrunk = same(layer, frame) && frame.is_some_and(|f| comp_fits(before, comp, f));
    log.step(
        "extract_last_cut",
        emptied && sequence_kept && shrunk && inside,
        json!({ "layer_secs": layer, "comp_secs": comp, "playhead_secs": shared.playhead() }),
    );
    shot(env, log, "2b-extract-last-cut");
    emptied
}

/// 空の sequence で、取り消し（カットとレイヤーの長さが戻る）とやり直し（Ctrl+Shift+Z。また空になる）を往復する。
/// また空になったら真。
fn redo_step(env: &Env<'_>, log: &mut Log) -> bool {
    let shared = env.shared;
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let restored = wait_until(WAIT, || cuts(shared).len() == 1);
    let [layer, ..] = lengths(shared);
    log.step(
        "undo_extract_last_cut",
        restored && same(layer, Some(MARK_OUT - MARK_IN)),
        json!({ "layer_secs": layer }),
    );
    if !restored {
        return false;
    }
    keys::dispatch(shared, "play-test", "Ctrl+Shift+KeyZ");
    let emptied = wait_until(WAIT, || cuts(shared).is_empty());
    let [layer, _, frame] = lengths(shared);
    log.step(
        "redo_extract_last_cut",
        emptied && same(layer, frame),
        json!({ "layer_secs": layer }),
    );
    emptied
}

/// 空の sequence へ `,` でもう一度 insert すると、sequence のレイヤーとコンポジションがカットの長さに追従する。
/// 取り消し 2 回（この insert と、やり直した extract）で、最初の insert の直後に戻ったら真。
fn reinsert_step(env: &Env<'_>, log: &mut Log, before: (usize, f64)) -> bool {
    let shared = env.shared;
    let length = MARK_OUT - MARK_IN;
    surface::post_key(env.surface_hwnd, VK_COMMA);
    let filled = wait_until(WAIT, || cuts(shared).len() == 1);
    let [layer, comp, _] = lengths(shared);
    log.step(
        "insert_into_empty_sequence",
        filled && same(layer, Some(length)) && comp_fits(before, comp, length),
        json!({ "layer_secs": layer, "comp_secs": comp }),
    );
    shot(env, log, "2c-insert-into-empty-sequence");
    if !filled {
        return false;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let emptied = wait_until(WAIT, || cuts(shared).is_empty());
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let restored = emptied && wait_until(WAIT, || cuts(shared).len() == 1);
    log.step("undo_to_first_insert", restored, json!({}));
    restored
}

/// 取り消し 1 回で、sequence も sequence のレイヤーも消え、コンポジションの長さも戻る。
fn undo_step(env: &Env<'_>, log: &mut Log, sequences: usize, before: (usize, f64)) {
    let shared = env.shared;
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || {
        shared.doc().is_some_and(|d| {
            sequence_of(&d.project).is_none()
                && d.project.sequences.len() == sequences
                && comp_shape(&d.project).is_some_and(|(_, s)| (s - before.1).abs() < EPS)
        })
    });
    log.step("undo_first_insert", undone, json!({}));
    shot(env, log, "3-first-insert-undo");
}

/// Source を開いてイン点とアウト点を付け、insert して、空の sequence の工程を通してから取り消す。
/// 空の sequence の工程の途中で止まったら、どこまで戻せばよいか分からないので取り消さない（通しの最後の restore が戻す）。
fn insert_and_undo(env: &Env<'_>, log: &mut Log, project: &Project) {
    let (sequences, before) = (project.sequences.len(), comp_shape(project));
    let (Some(before), Some(_)) = (before, mark_source(env, log)) else {
        return;
    };
    if !insert_step(env, log, before) {
        let why = json!("insert が適用されなかったので取り消さない");
        log.step("undo_first_insert", false, why);
        return;
    }
    if extract_step(env, log, before) && redo_step(env, log) && reinsert_step(env, log, before) {
        tools::run(env, log);
        undo_step(env, log, sequences, before);
    }
}

/// 最初のコンポジションにレイヤーがあるか。見出しの手順はレイヤーが無いと行えない。
pub fn has_layers(shared: &Shared) -> bool {
    let doc = shared.doc();
    doc.and_then(|d| comp_shape(&d.project))
        .is_some_and(|(n, _)| n > 0)
}

/// シーケンスのレイヤーが無いプロジェクトで、シーケンスを編集する手順の代わりに最初の insert の通しを `log` に足し、
/// 行わなかった手順の説明を返す。取り込んだ素材が無ければ insert の通しを行わない。
pub fn steps(env: &Env<'_>, log: &mut Log) -> String {
    let mut note =
        String::from("シーケンスのレイヤーが無いので、シーケンスを編集する手順をスキップした");
    match env.shared.doc().filter(|d| !d.project.assets.is_empty()) {
        Some(doc) => insert_and_undo(env, log, &doc.project),
        None => note.push_str("。素材も無いので、最初の insert の通しも行わない"),
    }
    if !has_layers(env.shared) {
        note.push_str("。レイヤーも無いので、見出しの手順も行わない");
    }
    note
}
