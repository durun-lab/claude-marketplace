## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 仕切り | サーフェスの中の、動かせる境界。サーフェスと WebView の境界の縦の仕切り（サーフェスの右端の幅 4 論理ピクセル。WebView の列を 1 つでも開けているときだけ出す）と、Program と Timeline の境界の横の仕切り（Program と Timeline の間隔）の 2 つ。ドラッグで動かし、つかんだまま Esc を押すと押す前に戻る | `Splitter`、`splitter_at`、`SPLITTER_W`、`GAP`、`win/splitter.rs` |
| Timeline の高さの割合 | サーフェスの高さに対する Timeline の高さ。横の仕切りが変え、割り付けの設定に保存する。描くときは Timeline と Program の下限を守る高さに収める | `timeline_ratio`、`TIMELINE_RATIO`、`timeline_height` |
| 割り付けの設定 | ユーザーが決める割り付け（WebView の幅、WebView の列の開け閉め、Timeline の高さの割合）。プロジェクトではなくユーザーの設定として `%LOCALAPPDATA%/movie-maker/workspace.json` に置く | `Workspace`、`WorkspaceSlot`、`workspace.rs`、`MM_WORKSPACE_FILE` |
| WebView の列の開け閉め | View > Left Panels と Right Panels で、左の列か右の列を出し入れすること。閉じた分だけサーフェスが広がる。閉じた列のパネルを選ぶコマンドは、先にその列を開ける。キーは無い | `view.toggle_left_column`、`view.toggle_right_column`、`LayoutCommand` |
| 列の幅と開閉（WebView に渡す） | アプリが WebView のシェルに渡す、右の列の幅（CSS の変数）と左右の列の開閉（`open` か `closed`）。割り付けを変えたときと、WebView がページを読み終えたとき（読み直しを含む）に送る | `layout::shell_script`、`--mm-right-w`、`data-mm-left`、`data-mm-right` |
| Reset UI Layout | 仕切りの位置と列の開け閉めを既定に戻す View のコマンド | `view.reset_ui_layout`、`LayoutCommand::ResetUiLayout` |
| 割り付けの工程 | `--play-test` の、仕切りのドラッグと Esc での取り消し、設定の書き出しと読み直し、列の開け閉めと View のチェック、閉じた列のパネルを選ぶコマンド、WebView の読み直し、Reset UI Layout を確かめる工程 | `verify_layout.rs` |

## 置き換えた語

### 割り付け（L3）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| スプリッター、ハンドル（仕切りの） | 仕切り。ハンドルは Timeline の帯の端のつまみ（`HANDLE_PX`）と紛れる | 仕切り |
| 帯（縦の仕切りの）、仕切りの帯 | サーフェスの右端の幅 4 論理ピクセルの縦の仕切り。用語集の帯（Timeline でレイヤーの範囲を表す矩形）と紛れる | 縦の仕切り、縦の仕切りの幅 |
| 隙間（Program と Timeline の） | 横の仕切りの場所。用語集の隙間（素材を持たないカット）と紛れる | Program と Timeline の間隔（`GAP`） |
| 比、高さの比（Timeline の） | `timeline_ratio`。1 字では何の比か分からず、同じ値を割合とも呼んでいた | Timeline の高さの割合 |
| 窓（の高さ） | ウィンドウの高さ。用語集で窓は区間とブロックに置き換えた語 | ウィンドウ |
| 版（設定のファイルの） | 設定のファイルの形式の `VERSION`。用語集で版はバージョンに置き換えた語 | バージョン |
| 試験 | 単体テストの HWND、`--play-test` の対象、`MM_WORKSPACE_FILE` を使う場面。用語集で試験はテストに置き換えた語 | テスト |
| 印（読み直す前のページの） | 読み直しの前後のページを見分ける JavaScript の変数。用語集で印はフラグとマーカーに置き換えた語 | フラグ |
| 当てる（列の幅と開閉を） | CSS の変数と属性を設定すること。用語集で当てるは適用するに置き換えた語 | 適用する |
| 道、開き直し | 書いた設定を読み直して割り付けを戻す経路 / アプリを閉じてもう一度起動すること。道は用語集で経路に置き換えた語で、開き直しは「開き直る」とも読める | 経路、再起動 |
| 符号（状態の） | `HOT` に置く 0、1、2。符号は正負とも読める | 番号（状態の） |
| 勝つ（角は縦が） | 縦と横の仕切りが交わる角で、縦の仕切りを当たりにすること | 優先する |
| 丸める（下限と上限に） | `clamp_to` と Timeline の高さの割合の下限と上限。物理ピクセルへの丸め（四捨五入）と紛れる | 収める |
| 壊れた数 | 設定のファイルの、有限でない数と 0 以下の数 | 有限でない数、0 以下の数 |
| 足す、引かれる（`WM_MOUSEMOVE` の） | サーフェスが縮んだときに Windows がカーソルの位置で `WM_MOUSEMOVE` を作ること / つかんだ位置が本物のカーソルの位置に動くこと | 合成する / 本物のカーソルの位置に動く |
| 見た目（列の、シェルに渡す） | WebView に渡す右の列の幅と列の開閉。見た目だけでは何を渡すか分からない | 列の幅と開閉（WebView に渡す） |
| 置き場所（設定のファイルの） | `%LOCALAPPDATA%/movie-maker/`。`WorkspaceSlot`（今の割り付けの置き場所）と紛れる | フォルダー |
| 割る（サーフェスを） | `split_viewers` が Program、Source、Timeline に分けること。割るは Split Clip でカットを割ることと紛れ、「比で割る」は割り算にも読める | 分ける |
| `Geo` | `splitter.rs` の、サーフェスと親のウィンドウの大きさの型。地理とも読める略 | `Geometry` |
| `bridge`（`layout/tests.rs` の変数） | 中継スクリプトの `EDIT_KEYS`。L2 で `BRIDGE` を `RELAY` に置き換えた | `relay` |
