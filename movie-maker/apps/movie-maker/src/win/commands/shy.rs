//! Shy のコマンド（After Effects の Shy）。印と隠すかどうかは表示の状態（`.mm/view.json`）に置くので、
//! 操作にならず取り消しの列に入らない。描画スレッドは Shy の状態の通し番号を見て Timeline の行を作り直す。

use crate::win::state::Shared;

/// Shy のコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShyCommand {
    /// 印の付いた行を Timeline から外す・戻す（Hide Shy Layers）。
    HideShyLayers,
    /// 選んだレイヤーの印を付ける・外す（Shy Selected Layers）。
    ShySelectedLayers,
}

/// 押せるかとチェックの状態を決める今の Shy の状態。
pub(super) struct ShyReady {
    /// 選んだレイヤーに印が付いているか。レイヤーを選んでいなければ `None`。
    selected: Option<bool>,
    /// 印の付いた行を隠している。
    hide: bool,
}

impl ShyReady {
    pub(super) fn of(shared: &Shared) -> Self {
        let shy = shared.view.shy();
        Self {
            selected: shared.selected_layer().map(|id| shy.layers.contains(&id)),
            hide: shy.hide,
        }
    }

    /// Shy Selected Layers はレイヤーを選んでいるときだけ押せる。
    pub(super) fn enabled(&self, command: ShyCommand) -> bool {
        command == ShyCommand::HideShyLayers || self.selected.is_some()
    }

    pub(super) fn checked(&self, command: ShyCommand) -> Option<bool> {
        match command {
            ShyCommand::HideShyLayers => Some(self.hide),
            ShyCommand::ShySelectedLayers => self.selected,
        }
    }
}

/// 表示の状態の Shy を変える。
pub(super) fn run(shared: &Shared, command: ShyCommand) {
    match command {
        ShyCommand::HideShyLayers => shared.view.toggle_hide_shy(),
        ShyCommand::ShySelectedLayers => {
            if let Some(id) = shared.selected_layer() {
                shared.view.toggle_shy(&id);
            }
        }
    }
}
