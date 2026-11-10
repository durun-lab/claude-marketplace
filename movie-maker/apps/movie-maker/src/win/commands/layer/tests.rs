use std::sync::Arc;

use mm_doc::{Layer, Node, Project};
use mm_ops::Op;
use mm_types::{Id, RationalTime};

use super::*;
use crate::win::commands::table;
use crate::win::sequence_edit::tests::sample_project;

/// シーケンスのレイヤー `l-seq`（最背面）の上に、文字のレイヤー `l-a`、`l-b` を積んだプロジェクト。
fn three_layers() -> Project {
    let mut p = sample_project();
    let secs = RationalTime::seconds;
    for id in ["l-a", "l-b"] {
        let node = Node::new("text", 1);
        p.compositions[0]
            .layers
            .push(Layer::new(id, node, secs(0), secs(0), secs(5)));
    }
    p
}

fn shared_with(selected: Option<&str>) -> Shared {
    let shared = Shared::new();
    shared.publish_doc(Arc::new(three_layers()), Id::new("main"));
    shared.select_layer(selected.map(str::to_owned));
    shared
}

fn state_of(shared: &Shared, id: &str) -> (bool, Option<bool>) {
    let table = table(shared);
    let state = table.iter().find(|c| c.id == id).expect(id).state;
    (state.enabled, state.checked)
}

#[test]
fn 重なりの順は_comp_の下から数え_前へは_1_つ大きい位置_後ろへは_1_つ小さい位置へ動かす() {
    let p = three_layers();
    let comp = &p.compositions[0];
    let index_of = |op: Result<Op, String>| match op.expect("操作") {
        Op::MoveLayer(m) => m.index,
        other => panic!("{other:?}"),
    };
    assert_eq!(index_of(op_of(LayerCommand::MoveUp, comp, "l-seq")), 1);
    assert_eq!(index_of(op_of(LayerCommand::MoveDown, comp, "l-b")), 1);
    assert_eq!(
        op_of(LayerCommand::MoveUp, comp, "l-b"),
        Err("Already at the front".to_owned())
    );
    assert_eq!(
        op_of(LayerCommand::MoveDown, comp, "l-seq"),
        Err("Already at the back".to_owned())
    );
}

#[test]
fn 有効とロックは今の状態を反転する操作になる() {
    let p = three_layers();
    let comp = &p.compositions[0];
    match op_of(LayerCommand::ToggleEnabled, comp, "l-a").expect("操作") {
        Op::SetLayerEnabled(op) => assert!(!op.enabled, "有効なレイヤーは無効にする"),
        other => panic!("{other:?}"),
    }
    match op_of(LayerCommand::ToggleLock, comp, "l-a").expect("操作") {
        Op::LockLayer(op) => assert!(op.lock, "ロックしていないレイヤーはロックする"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn コンポジションに無いレイヤーは操作にしない() {
    let p = three_layers();
    let result = op_of(LayerCommand::ToggleLock, &p.compositions[0], "nope");
    assert!(result.is_err());
}

#[test]
fn レイヤーを選ぶまでレイヤーのコマンドは押せない() {
    let shared = shared_with(None);
    for id in [
        "layer.toggle_enabled",
        "layer.toggle_lock",
        "layer.move_up",
        "layer.move_down",
    ] {
        assert_eq!(state_of(&shared, id), (false, None), "{id}");
    }
}

#[test]
fn 選んだレイヤーの有効とロックをチェックで出し_先頭と最後では動かす向きを押せない() {
    let shared = shared_with(Some("l-b"));
    assert_eq!(
        state_of(&shared, "layer.toggle_enabled"),
        (true, Some(true))
    );
    assert_eq!(state_of(&shared, "layer.toggle_lock"), (true, Some(false)));
    assert!(!state_of(&shared, "layer.move_up").0, "先頭");
    assert!(state_of(&shared, "layer.move_down").0);
    shared.select_layer(Some("l-seq".to_owned()));
    assert!(state_of(&shared, "layer.move_up").0);
    assert!(!state_of(&shared, "layer.move_down").0, "最後");
}
