## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| sequence の自動作成 | sequence のレイヤーが無いコンポジションへの Insert と Overwrite、プロジェクトに sequence が無いときの Story の Arrange が、空の sequence（`Sequence <番号>`）と、それを置く sequence のレイヤー（`Sequence <番号> layer`）を足してからカットを入れること。1 つのトランザクションにする | `NewSequence`、`plan_new_sequence`、`arrange_ops`、`story_arrange` |

## 置き換えた語

### UI の分かりやすさ（sequence の自動作成）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 最初の編集 | sequence のレイヤーが無いコンポジションへの Insert、Overwrite と、sequence が無いプロジェクトの Arrange が sequence を作ること。何の最初かが分からず、2 回目の編集でも sequence のレイヤーが無ければ作る | sequence の自動作成 |
| 列（操作の） | `Planned::ops` と `NewSequence::ops` の説明。用語集の「列」は「並び」などに置き換えた語 | 並び |
| 断る | Split、Lift、Extract と Arrange が受け付けないこと。用語集の「断る」は「拒否する」に置き換えた語 | 拒否する |
| 頼み（編集の） | Story のパネルがアプリに送る編集（`Loaded::request`）。選択の頼み（DOM のイベント）と紛れる | 依頼 |
| 入る先 | Insert がカットを入れる sequence のレイヤー。`target_layer` の説明の語は「対象」 | 対象（編集の） |
| 重なる（ID が） | 同じ ID があること。`Project::validate` の説明の語は「重複」で、触る所が重なることとも紛れる | 重複する |
| 段（`--play-test` の） | 通しの各部分。`verify_stage` の語は「工程」 | 工程 |
| 主のトラック、主のストーリーライン | CapCut と FCP の、最初のクリップが入る所。製品の日本語の名前と違う | メイントラック、基本ストーリーライン |
| 形式（FCP の） | FCP が最初のクリップで決める大きさとフレームレート | ビデオのプロパティ |
| 保存の層 | 試験の文言。`Store` が適用の前に通す関数のこと | `with_fitted_layers` |
| 黒（カットの後ろの） | コンポジションの長さがカットより長いときに映る所 | 黒い画面 |
