## プロジェクトのデータと操作

| 用語 | 定義 | 識別子 |
|---|---|---|
| 拒否の文 | 操作を拒否した理由の英語の 1 文。アプリの Shell のエラーの表示、CLI、MCP と HTTP の `message` に同じ文が出る。書き方は 05-ops.md の「操作」 | `MmError`、`OpError` の `Display`、`.mm-shell-error` |
| 拒否の文の語 | 拒否の文で正式な用語を呼ぶ英語。素材 → asset、カット → clip、イン点 / アウト点 → in point / out point（Source で素材に付けたものは in/out points）、入れ子のコンポジションを置いたレイヤー → nested composition layer、sequence を置いたレイヤー → sequence layer、参照のショート → reference short、元のカット → origin clip、選別 → selection、話題 → topic、字幕の置き場所 → caption placement、ノードの種類 → node kind、操作 → operation。文の頭では大文字で始める | `mm-doc`、`mm-ops`、`mm-store` の拒否の文 |
| asset（拒否の文の） | 拒否の文で素材を呼ぶ語。操作の名前と引数（`addAsset`、`asset`）と同じ語にし、同じ文に出るカット（clip）と分ける。画面の文言は Media の素材も Clip と呼ぶ | `Asset {} already exists`、`<参照する側>: asset {} not found` |

## 置き換えた語

### 操作を拒否した理由を英語に（E2）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| touch | ロックの拒否の文（`touches` はコードの中の語） | edit（`Can't edit locked items`） |
| Edit overlaps | 衝突の拒否の文の頭。命令形の Edit に読める | Conflicts with |
| op | 提案の承認の拒否の文（`Op index`、`No ops approved`） | operation |
| cut | カットを ID で引いて見つからないときの文（`Not found: cut c1`） | clip |
| caption slot | 字幕の置き場所が無いときの文 | caption placement |
| in/out marks | Source で素材に付けたイン点とアウト点 | in/out points |
| source | `trimLayer` が素材の余白を越えるときの文（`beyond its source`） | asset |
| range start / range end | `forkSequence` の範囲の両端のカット | first clip / last clip |
| has no sequence | `fitLayer` で sequence を置いていないレイヤー（`setReframe` と同じ状態） | is not a sequence layer |
| nested composition（レイヤーを指して） | `unprecompose` で入れ子のコンポジションを置いていないレイヤー | nested composition layer |
| 引きそこね | 05-ops.md の拒否の文の書き方 | ID で引いて見つからないとき |
| 持ち主 | 検証で参照先が無いときの文の頭 | 参照する側 |
| 画面の知らせ | 取り消しの記録の名前に `Undo:` を付ける側 | 編集の文言 |
