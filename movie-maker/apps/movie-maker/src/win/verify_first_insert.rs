//! `--play-test` の、sequence のレイヤーが無いコンポジションへの最初の insert の通し（`mm new` の後に素材を取り込んだ形）。
//! 実際のウィンドウへのキーで Source にイン点とアウト点を付けて `,` を押し、sequence と sequence のレイヤーとカットが入ることと、
//! 取り消し 1 回で全部消えることを確かめ、工程ごとにスクリーンショットを残す。

use mm_doc::{Project, SEQUENCE_KIND};
use serde_json::json;

use crate::win::keys;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::verify_edit::{
    cuts, mark_source, sequence_of, shot, Env, Log, MARK_IN, MARK_OUT, VK_COMMA, WAIT,
};
use crate::win::verify_stage::wait_until;

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

/// insert の後のコンポジションの長さが決まりどおりか。レイヤーが無かったらカットの長さ、ほかのレイヤーがあれば縮まない。
fn comp_fits(before: (usize, f64), after: f64, cut: f64) -> bool {
    match before {
        (0, _) => (after - cut).abs() < EPS,
        (_, secs) => after > secs - EPS,
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
    let layer = shared.doc().and_then(|d| layer_secs(&d.project));
    let comp = shared
        .doc()
        .and_then(|d| comp_shape(&d.project))
        .map(|(_, s)| s);
    let fitted = layer.is_some_and(|s| (s - length).abs() < EPS);
    let comp_ok = comp.is_some_and(|s| comp_fits(before, s, length));
    log.step(
        "first_insert",
        made && ranged && fitted && comp_ok,
        json!({ "cut": cut.map(|c| c.id.to_string()), "layer_secs": layer, "comp_secs": comp }),
    );
    shot(env, log, "2-first-insert");
    made
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

/// Source を開いてイン点とアウト点を付け、insert して取り消す。
fn insert_and_undo(env: &Env<'_>, log: &mut Log, project: &Project) {
    let (sequences, before) = (project.sequences.len(), comp_shape(project));
    if let (Some(before), Some(_)) = (before, mark_source(env, log)) {
        if insert_step(env, log, before) {
            undo_step(env, log, sequences, before);
        } else {
            let why = json!("insert が適用されなかったので取り消さない");
            log.step("undo_first_insert", false, why);
        }
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
