//! 割り付けのコマンド。View の WebView の列の開け閉め（Left Panels、Right Panels）と Reset UI Layout。割り付けは
//! ユーザーの設定（`workspace.rs`）で、操作にならず取り消しのスタックに入らない。変えたらウィンドウの配置を直し、設定に書く。

use crate::win::layout;
use crate::win::state::Shared;
use crate::win::workspace::Workspace;

/// 割り付けのコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutCommand {
    /// 左の列（Tasks、Media、Story、Transitions、Mixer、Deliver）を開け閉めする。
    ToggleLeftColumn,
    /// 右の列（Inspector、Review、Devices、Feedback、Diagnostics）を開け閉めする。
    ToggleRightColumn,
    /// 仕切りの位置と列の開け閉めを既定に戻す。
    ResetUiLayout,
}

/// 左の列に並ぶパネルの id。残りのパネルは右の列。`ui/panels/*/src/panel.ts` の `column` と同じにし、テストで突き合わせる。
const LEFT_PANELS: &[&str] = &["deliver", "media", "mixer", "story", "tasks", "transition"];

/// コマンドのチェックの状態。列の開け閉めは今開いているか。Reset UI Layout は入/切が無い。
pub(super) fn checked(workspace: &Workspace, command: LayoutCommand) -> Option<bool> {
    match command {
        LayoutCommand::ToggleLeftColumn => Some(workspace.left_open),
        LayoutCommand::ToggleRightColumn => Some(workspace.right_open),
        LayoutCommand::ResetUiLayout => None,
    }
}

/// 割り付けを変えて設定に書き、ウィンドウの配置を直す。
pub(super) fn run(shared: &Shared, command: LayoutCommand) {
    shared.workspace.update(|w| match command {
        LayoutCommand::ToggleLeftColumn => w.left_open = !w.left_open,
        LayoutCommand::ToggleRightColumn => w.right_open = !w.right_open,
        LayoutCommand::ResetUiLayout => *w = Workspace::default(),
    });
    shared.workspace.save();
    layout::refresh();
}

/// パネルを選ぶコマンドの前に、そのパネルの列を開ける。閉じた列のタブを選んでも何も見えないため。
pub(super) fn reveal(shared: &Shared, panel: &str) {
    let left = LEFT_PANELS.contains(&panel);
    let hidden = shared.workspace.get();
    if if left { hidden.left_open } else { hidden.right_open } {
        return;
    }
    shared.workspace.update(|w| {
        if left {
            w.left_open = true;
        } else {
            w.right_open = true;
        }
    });
    shared.workspace.save();
    layout::refresh();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 左の列のパネルは_panel_ts_の_column_と同じ() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/panels");
        let mut left = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let file = entry.unwrap().path().join("src/panel.ts");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let field = |key: &str| {
                text.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix(&format!("{key}: \""))?
                        .split('"')
                        .next()
                        .map(str::to_owned)
                })
            };
            let (id, column) = (field("id").unwrap(), field("column").unwrap());
            if column == "left" {
                left.push(id);
            }
        }
        left.sort();
        assert_eq!(left, LEFT_PANELS);
    }

    #[test]
    fn 列のコマンドは今の開閉をチェックに出し_リセットには出さない() {
        let mut w = Workspace::default();
        w.right_open = false;
        assert_eq!(checked(&w, LayoutCommand::ToggleLeftColumn), Some(true));
        assert_eq!(checked(&w, LayoutCommand::ToggleRightColumn), Some(false));
        assert_eq!(checked(&w, LayoutCommand::ResetUiLayout), None);
    }
}
