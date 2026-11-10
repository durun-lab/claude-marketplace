## 時間とシーケンス

| 用語 | 定義 | 識別子 |
|---|---|---|
| 空の sequence | 有効なカットが無い sequence。最後のカットを抜いても（`extract`、`extractCuts`、`cutWords`）、全部のカットを無効にしても（`disable`）消えずに残り、それを置くレイヤーは長さ 1 フレームになる。次の Insert と Overwrite は、残した sequence とレイヤーに入れる | `fitted_length`、`plan_overwrite` |

## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| sequence に合わせる長さ（レイヤーの） | sequence を置いたレイヤーが追従する長さ。sequence の長さと、コンポジションの `fps` の 1 フレームの長い方。レイヤーの長さが、操作の前のこの長さと同じなら追従しているとみなす | `fitted_length`、`with_fitted_layers` |

## 置き換えた語

### UI の分かりやすさ（最後のカットの Extract）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 当てる（1 フレームを） | 空の sequence を置くレイヤーの長さ。用語集の「当てる」は「適用する」と「並べる」に置き換えた語 | 1 フレームのレイヤーを残す |
| 逆（1 回の逆で戻る） | 試験の名前。逆操作の略 | 逆操作（逆操作 1 回で戻る） |
| `remove`（カットを抜く操作） | 設計書で sequence を空にする操作に挙げた名前。その名前の操作は無い | `extractCuts`、`cutWords`（と、全部のカットの `disable`） |
| 空のトラック（Premiere と Resolve の） | 最後のクリップを Ripple Delete した後に残るもの。残るのは sequence で、トラックではない | 空の sequence（Resolve は Timeline） |
| 列（操作の） | `Store::apply_txn` に渡す操作の並び。用語集の「列」は「並び」などに置き換えた語 | 並び |
| 1 フレーム（コンポジションの `fps`） | 括弧が何を指すかが分からない | コンポジションの `fps` の 1 フレーム |
| `back`、`kept`、`fitted`（通しの変数） | 取り消しでカットが戻ったか、sequence が残ったか、1 フレームに縮んだか。何がかが名前に無く、`fitted` は sequence に合わせる長さ（`fitted_length`）と紛れる | `restored`、`sequence_kept`、`shrunk` |
| `2b-extract-last`、`2c-insert-into-empty`（スクリーンショットの名前） | 何の最後か、何が空かが名前に無い。工程の名前は `extract_last_cut` と `insert_into_empty_sequence` | `2b-extract-last-cut`、`2c-insert-into-empty-sequence` |
