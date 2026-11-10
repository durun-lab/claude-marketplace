//! `--play-test` の、sequence のレイヤーが無いコンポジションへの最初の insert の通し（`mm new` の後に素材を取り込んだ形）。
//! 実際のウィンドウへのキーで Source にイン点とアウト点を付けて `,` を押し、sequence と sequence のレイヤーとカットが入ることと、
//! 取り消し 1 回で全部消えることを確かめ、段ごとにスクリーンショットを残す。

use mm_doc::{Project, SEQUENCE_KIND};
use serde_json::{json, Value};

use crate::win::keys;
use crate::win::surface;
use crate::win::verify_edit::{
    cuts, mark_source, sequence_of, shot, Env, Log, MARK_IN, MARK_OUT, VK_COMMA, WAIT,
};
use crate::win::verify_stage::wait_until;

/// 最初のコンポジションの sequence のレイヤーの長さ（秒）。
fn layer_secs(project: &Project) -> Option<f64> {
    let comp = project.compositions.first()?;
    let layer = comp
        .layers
        .iter()
        .find(|l| l.source.kind == SEQUENCE_KIND)?;
    Some(layer.duration().ok()?.to_f64())
}

/// `,` で、Source の範囲のカット 1 つの sequence と、カットの長さの sequence のレイヤーができる。取り消し 1 回で全部消える。
fn insert_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let sequences = shared.doc().map_or(0, |d| d.project.sequences.len());
    surface::post_key(env.surface_hwnd, VK_COMMA);
    let made = wait_until(WAIT, || cuts(shared).len() == 1);
    let cut = cuts(shared).into_iter().next();
    let ranged = cut.as_ref().is_some_and(|c| {
        (c.in_point.to_f64() - MARK_IN).abs() < 1e-6
            && (c.out_point.to_f64() - MARK_OUT).abs() < 1e-6
    });
    let layer = shared.doc().and_then(|d| layer_secs(&d.project));
    let fitted = layer.is_some_and(|s| (s - (MARK_OUT - MARK_IN)).abs() < 1e-6);
    log.step(
        "first_insert",
        made && ranged && fitted,
        json!({ "cut": cut.map(|c| c.id.to_string()), "layer_secs": layer }),
    );
    shot(env, log, "2-first-insert");
    if !made {
        log.step(
            "undo_first_insert",
            false,
            json!("insert が適用されなかったので取り消さない"),
        );
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || {
        shared.doc().is_some_and(|d| {
            sequence_of(&d.project).is_none() && d.project.sequences.len() == sequences
        })
    });
    log.step("undo_first_insert", undone, json!({}));
    shot(env, log, "3-first-insert-undo");
}

/// 最初の insert の通しを行って結果を返す。取り込んだ素材が無ければスキップする。
pub fn run(env: &Env<'_>) -> Value {
    let no_asset = env.shared.doc().is_none_or(|d| d.project.assets.is_empty());
    if no_asset {
        return json!({ "skipped": "シーケンスのレイヤーも素材も無い" });
    }
    let mut log = Log::default();
    if mark_source(env, &mut log).is_some() {
        insert_step(env, &mut log);
    }
    log.into_json()
}
