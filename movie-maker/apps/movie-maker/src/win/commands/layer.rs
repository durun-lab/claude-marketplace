//! 選んだレイヤーのコマンド（Enable Layer、Lock Layer、Move Layer Up、Move Layer Down）。右クリックのメニューが、見出しと帯を
//! 右クリックして選んだレイヤーに対して呼ぶ。操作（`setLayerEnabled`、`lockLayer`、`moveLayer`）にして編集のキューで適用する。

use std::sync::Arc;

use mm_doc::Composition;
use mm_ops::{MoveLayer, Op};
use mm_types::Id;
use mm_ui_layout::HeaderButton;

use crate::win::edit;
use crate::win::sequence_edit::header_button_op;
use crate::win::state::Shared;

/// レイヤーのコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCommand {
    /// 有効と無効を切り替える（Enable Layer）。
    ToggleEnabled,
    /// ロックを切り替える（Lock Layer）。
    ToggleLock,
    /// 1 つ前へ出す（Move Layer Up）。Timeline では 1 行上へ動く。
    MoveUp,
    /// 1 つ後ろへ回す（Move Layer Down）。Timeline では 1 行下へ動く。
    MoveDown,
}

/// 選んでいるレイヤーの今の状態。押せるかとチェックの状態を決める。
#[derive(Default)]
pub(super) struct LayerReady(Option<Flags>);

struct Flags {
    enabled: bool,
    lock: bool,
    /// コンポジションの中の位置（下から数える。大きいほど前）と、レイヤーの数。
    index: usize,
    count: usize,
}

impl LayerReady {
    pub(super) fn of(shared: &Shared) -> Self {
        let flags = shared.selected_layer().and_then(|id| {
            let doc = shared.doc()?;
            let comp = doc.project.composition(&doc.comp).ok()?;
            let index = comp.layers.iter().position(|l| l.id.as_str() == id)?;
            let layer = &comp.layers[index];
            Some(Flags {
                enabled: layer.enabled,
                lock: layer.lock,
                index,
                count: comp.layers.len(),
            })
        });
        Self(flags)
    }

    /// レイヤーを選んでいるときだけ押せる。Move Layer Up は先頭、Move Layer Down は最後のレイヤーでは押せない。
    pub(super) fn enabled(&self, command: LayerCommand) -> bool {
        let Some(f) = &self.0 else { return false };
        match command {
            LayerCommand::MoveUp => f.index + 1 < f.count,
            LayerCommand::MoveDown => f.index > 0,
            LayerCommand::ToggleEnabled | LayerCommand::ToggleLock => true,
        }
    }

    /// Enable Layer と Lock Layer は今の状態を入/切で出す。
    pub(super) fn checked(&self, command: LayerCommand) -> Option<bool> {
        let f = self.0.as_ref()?;
        match command {
            LayerCommand::ToggleEnabled => Some(f.enabled),
            LayerCommand::ToggleLock => Some(f.lock),
            LayerCommand::MoveUp | LayerCommand::MoveDown => None,
        }
    }
}

/// 選んでいるレイヤーに操作を適用する。計画に使ったデータのバージョンを付けて編集のキューで行い、失敗は文言で出す。
pub(super) fn run(shared: &Arc<Shared>, command: LayerCommand) {
    let s = shared.clone();
    edit::queue(shared, move || {
        let result = apply(&s, command);
        s.edit.set_message(result.err().unwrap_or_default());
    });
}

fn apply(shared: &Shared, command: LayerCommand) -> Result<(), String> {
    let id = shared.selected_layer().ok_or("No layer selected")?;
    let doc = shared.doc().ok_or("No project open")?;
    let comp = doc
        .project
        .composition(&doc.comp)
        .map_err(|e| e.to_string())?;
    let op = op_of(command, comp, &id)?;
    edit::submit_based(shared, vec![op], doc.version).map(drop)
}

/// コマンドを、コンポジション `comp` のレイヤー `id` への操作にする。
fn op_of(command: LayerCommand, comp: &Composition, id: &str) -> Result<Op, String> {
    let index = comp
        .layers
        .iter()
        .position(|l| l.id.as_str() == id)
        .ok_or("The layer isn't in this composition")?;
    let layer = &comp.layers[index];
    let to = match command {
        LayerCommand::ToggleEnabled => return Ok(header_button_op(HeaderButton::Enabled, layer)),
        LayerCommand::ToggleLock => return Ok(header_button_op(HeaderButton::Lock, layer)),
        LayerCommand::MoveUp => index + 1,
        LayerCommand::MoveDown => index.checked_sub(1).ok_or("Already at the back")?,
    };
    if to >= comp.layers.len() {
        return Err("Already at the front".into());
    }
    Ok(MoveLayer {
        layer: Id::new(id),
        index: to,
    }
    .into())
}

#[cfg(test)]
mod tests;
