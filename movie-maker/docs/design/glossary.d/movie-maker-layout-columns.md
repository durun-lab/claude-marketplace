## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 橋（WebView の） | アプリが WebView に入れる、画面を持たないスクリプト。WebView にフォーカスがあるときのキーをキーの表へ送り、Rust の再生位置の通知と Timeline の選択を shell の DOM のイベントにつなぎ、パネルが投げる素材の選択とシークの頼みをアプリに送る | `apps/movie-maker/web/transport.js` |
| 編集の結果の文言 | 直近の編集を断った理由か、取り消したもの。Shell がステータスの行に出し、次の編集が成功すると消える。断った理由は拒否の色で出す | `EditMessage`、`EditState::set_message`、`EditState::notify`、`mm:edit-message`、`onEditMessage` |
| ステータスの行（WebView の） | WebView の下端の、2 つの列にまたがる 1 行。編集の結果の文言を、文言のある間だけ出す。サーフェスの下端のステータスバー（Z5）とは別 | `StatusLine`、`.mm-shell-status`、`.mm-shell-message` |
| サーフェスの幅の下限 | ウィンドウの幅の下限を決めるために、サーフェスに残す幅（400 論理ピクセル）。WebView の 2 列は幅を変えないので、ウィンドウの幅の下限はこれと 2 列の幅の和 | `MIN_SURFACE_W` |
| パネルを選ぶコマンド | View のパネルの項目。タブを押したのと同じにパネルのタブを選び、パネルの `requests` は進めない。ID は `view.show_<パネルの id>`、持ち主はそのパネル | `view.show_media`、`show_panel!` |
| WebView の工程 | `--play-test` の、WebView の中でイベントを投げて結果を `eval_with_callback` で読む工程。橋、2 列の割り付け、Diagnostics、Inspector のキーフレームの欄、編集の結果の文言を確かめる | `verify_webview.rs` |

## 置き換えた語

### 割り付け（L2）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 再生の操作列 | `transport.js` と、それを指すコメント。画面を持たなくなり、再生の操作のバーとも紛れる | 橋（WebView の） |
