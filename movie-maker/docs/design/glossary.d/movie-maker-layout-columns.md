## 構造と開発

| 用語 | 定義 | 識別子 |
|---|---|---|
| 中継スクリプト（WebView の） | アプリが WebView に入れる、画面を持たないスクリプト。WebView にフォーカスがあるときのキーをキーの表へ送り、Rust の再生位置の通知と Timeline の選択を shell の DOM のイベントにつなぎ、パネルが投げる素材の選択とシークの頼みをアプリに送る | `apps/movie-maker/web/transport.js`、`TRANSPORT_JS`、`ui/shell/src/relay.test.ts` |
| 編集の結果の文言 | 直近の編集を拒否した理由か、取り消したもの。Shell がステータスの行に出し、次の編集が成功すると消える。拒否の理由は拒否の文と同じ色で出す | `EditMessage`、`EditState::set_message`、`EditState::notify`、`mm:edit-message`、`onEditMessage` |
| ステータスの行（WebView の） | WebView の下端の、2 つの列にまたがる 1 行。編集の結果の文言を、文言のある間だけ出す。サーフェスの下端のステータスバー（Z5）とは別 | `StatusLine`、`.mm-shell-status`、`.mm-shell-message` |
| サーフェスの幅の下限 | 縦の仕切りで WebView を広げても、ウィンドウが狭くても、Program と Timeline に残す幅（400 論理ピクセル。縦の仕切りの幅は別）。ウィンドウの幅の下限は、これと 2 列の既定の幅と縦の仕切りの幅の和 | `MIN_SURFACE_W` |
| パネルを選ぶコマンド | View のパネルの項目。タブを押したのと同じにパネルのタブを選び、パネルの `requests` は進めない。ID は `view.show_<パネルの id>`、持ち主はそのパネル | `view.show_media`、`show_panel!` |
| WebView の工程 | `--play-test` の、WebView の中でイベントを投げて結果を `eval_with_callback` で読む工程。中継スクリプト、2 列の割り付け、Diagnostics、Inspector のキーフレームの欄、編集の結果の文言を確かめる | `verify_webview.rs` |

## 置き換えた語

### 割り付け（L2）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 再生の操作列 | `transport.js` と、それを指すコメント。画面を持たなくなり、再生の操作のバーとも紛れる | 中継スクリプト（WebView の） |
| 橋（WebView の）、橋、`BRIDGE`、`bridge.test.ts` | `transport.js` と、それを指すコメント、テストの名前、設計書、全体図。1 字の比喩で、用語集のブリッジ（C++ を C に見せる薄い層）とも紛れる | 中継スクリプト（WebView の）、`RELAY`、`relay.test.ts` |
| 断る、断った理由 | 編集を拒否した理由（`EditMessage::failed`）。用語集で断るは拒否するに置き換えた語 | 拒否する、拒否の理由 |
| 試験 | jsdom のテスト、Tauri の外のデモとテスト、再生テストの表示の状態。用語集で試験はテストに置き換えた語 | テスト |
| 地の色 | 取り消しの知らせの文字の色。地は背景とも読める | 通常の文字の色（`text`） |
| 光らせる | 文言を置き直したときの背景のアニメーション。ライトのモードでは暗い色から戻るので、明るくなるとは限らない | 背景の色を一瞬変える |
| 印（繰り返しの） | `key_event` の `repeat`。用語集で印はフラグに置き換えた語 | フラグ（繰り返しの） |
| 欄（GPU の） | Diagnostics の GPU の名前と値の組。用語集で欄（単体）は入力欄とフィールドに置き換えた語 | 行（GPU の） |
| 境目（領域の） | サーフェスと WebView の間。用語集で境目は境界に置き換えた語 | 境界 |
| 画（前の） | スクリーンショットに写る、前に描いた画面。画は単体ではサムネイルやフレームと紛れる | 画面 |
| 頼み（パネルの） | パネルを選ぶコマンドが進めないもの（Media の取り込みなど） | パネルの `requests` |
| `takeTiming` | 選択の中身のコンポジションの要約から fps と長さを取り直す関数。timing が何を指すかが名前に無い | `takeFpsAndDuration` |
| `emit_message_if_changed`、`sent_message` | 編集の結果の文言を WebView に送る関数と、送った番号。何の文言かが名前に無い | `emit_edit_message_if_changed`、`sent_edit_message` |
