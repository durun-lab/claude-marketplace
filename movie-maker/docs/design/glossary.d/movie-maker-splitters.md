## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 仕切り | サーフェスの中の、動かせる境界。サーフェスと WebView の境界（縦。サーフェスの右端の帯）と、Program と Timeline の境界（横）の 2 つ。ドラッグで動かす | `Splitter`、`splitter_at`、`win/splitter.rs` |
| 割り付けの設定 | ユーザーが決める割り付け（WebView の幅、WebView の列の開け閉め、Timeline の高さの割合）。プロジェクトではなくユーザーの設定として `%LOCALAPPDATA%/movie-maker/workspace.json` に置く | `Workspace`、`WorkspaceSlot`、`workspace.rs`、`MM_WORKSPACE_FILE` |
| WebView の列の開け閉め | View > Left Panels と Right Panels で、左の列か右の列を出し入れすること。閉じた分だけサーフェスが広がる | `view.toggle_left_column`、`view.toggle_right_column`、`LayoutCommand` |
| Reset UI Layout | 仕切りの位置と列の開け閉めを既定に戻す View のコマンド | `view.reset_ui_layout`、`LayoutCommand::ResetUiLayout` |
| 割り付けの工程 | `--play-test` の、仕切りのドラッグ、設定の書き出しと読み直し、列の開け閉め、Reset UI Layout を確かめる工程 | `verify_layout.rs` |

## 置き換えた語

### 割り付け（L3）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| スプリッター、ハンドル（仕切りの） | 仕切り。ハンドルは Timeline の帯の端のつまみ（`HANDLE_PX`）と紛れる | 仕切り |
