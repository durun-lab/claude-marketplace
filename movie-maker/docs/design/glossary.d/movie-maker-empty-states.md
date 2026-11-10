## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 空の状態 | 画面の場所に出すものが無いときの状態。Program と Timeline はコンポジションに中身が無いとき、Inspector は何も選んでいないとき、Story は文字起こしが無いとき | `EmptyState`、`empty_state`、`EmptyInspector.tsx` |
| 空の状態の案内 | 空の状態の場所に出す、次にすることとキーの文。Program の取り込みの案内と Timeline の行の領域の 3 点編集の手順はサーフェスに描き、キーはコマンドの表から引く | `EmptyHints`、`EmptyKeys`、`mm-insp-empty-hint`、`mm-story-empty` |
| 中身（コンポジションの） | 空の状態の判定で数えるもの。sequence でないレイヤーと、シーケンスのカット（無効のレイヤーとカットを含む）。Shy で隠した行も数える | `empty_state` |
| コンポジションの要約 | 何も選んでいない Inspector に出す、名前（コンポジションの ID）、大きさ、fps、長さ（タイムコード）、レイヤーの数 | `CompositionSummary`、`LayerSelection::composition` |
| 選択の頼み | Shell が選択のイベントを聞き始めたときに投げる DOM のイベント。`transport.js` が選択の Tauri イベントを購読し終えてから、今の選択を読んで選択のイベントで答える | `mm:layer-selection-request`、`layer_selection` |

## 置き換えた語

### UI の分かりやすさ（D4、空の状態）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| レイヤーが無い（案内の条件） | 空の状態の案内を出す条件。カットの無い sequence のレイヤーだけのときも含み、無効のレイヤーだけのときは含まない | コンポジションに中身が無い |
| 次の一歩 | 空の状態に出す文。比喩で、何を出すかが分からない | 次にすること |
| `EmptyHints`（キーだけを持つ型） | 案内に出すキー。どの案内を出すかを合わせて持つ型（`EmptyHints`）と紛れる | `EmptyKeys` |
| `hints`、`set_hints`、`empty_hints` | `Ui` のフィールドと関数、アプリでキーを作る関数。Inspector の `hints`（`ParamHints`）と紛れる | `empty`、`set_empty_keys`、`empty_keys` |
| `empty_program_labels`、`empty_timeline_labels` | 案内の文字を置く関数。どの案内を出すかを持つ型のメソッドにした | `EmptyHints::program`、`EmptyHints::timeline` |
| 見出し、`TITLE_SCALE`（Program の案内の） | Program の案内の 1 行目。レイヤーの見出しと見出しの列と紛れる | 1 行目、`FIRST_LINE_SCALE` |
| `layers`（コンポジションの要約の） | レイヤーの数。`ProjectInfo::layers`（レイヤーの一覧）と紛れる | `layer_count` |
| `mm-insp-hint` | 空の Inspector の案内の段落。Inspector の `hints`（`ParamHints`）と紛れる | `mm-insp-empty-hint` |
| 引く、`pullSelection`（今の選択を） | IPC で今の選択を読むこと。用語集の「引く」は表から探すこと | 読む、`readSelection` |
| `rows`（`EmptyInspector.tsx` の） | コンポジションの要約の行。Timeline のレイヤーの行と紛れる | `summaryRows` |
| 境目（フレームの） | `FRAME_EPS` の説明。用語集の「境目」は「境界」に置き換えた語 | 境界（フレームの） |
