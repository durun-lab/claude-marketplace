## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| コマンド（UI の） | 画面の操作の入口 1 つ。UI のコマンドの表の 1 行で、Tasks の項目とキーの表が ID（`<組>.<動詞>`）で指す。操作（`Op`）とは別 | `commands::Spec`、`UiCommand` |
| UI のコマンドの表 | UI のコマンドを 1 つにまとめた Rust の表。ID、名前、キー、説明、メニュー、持ち主、押せるかの列を持ち、WebView は IPC で読む | `commands.rs`、`command_table`、`run_command`、`mm:commands-changed` |
| 持ち主（UI のコマンドの） | UI のコマンドを実行する所。アプリ（Rust）か、タブを開く WebView のパネル | `commands::Owner`、`UiCommand.panel` |
| 押せる（UI のコマンドが） | いま実行できること。押せないコマンドと表に無いコマンドの Tasks の項目は薄い色で出す | `commands::State`、`UiCommand.enabled` |
| キーの見た目 | キーの表の名前を画面に出す形にしたもの（`Ctrl+KeyZ` は `Ctrl+Z`、`ArrowLeft` は `←`） | `keys::label`、`UiCommand.key_label` |
