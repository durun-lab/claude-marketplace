## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 見出しのボタン | レイヤーの見出しの有効、ロック、Mute、Solo のボタン。押すとレイヤーの今の状態を反転する 1 回の操作（`setLayerEnabled`、`lockLayer`、`setLayerAudio`）になり、取り消し 1 回で戻る。レイヤーは選ばない。ロックしたレイヤーでも通す | `HeaderButton`、`Hit::HeaderButton`、`Action::HeaderButton`、`header_button_op` |
| 見出しのドラッグ | レイヤーの見出しを縦にドラッグして重なりの順を変えること。4 論理ピクセル動くまではクリックとして選ぶだけ。離したとき 1 回の `moveLayer` にし、Esc で取りやめる | `HeaderDrag`、`Drag::Header`、`Action::MoveLayer`、`move_layer_op` |
| 行の境界（見出しのドラッグの） | 隣り合う行の間の位置。番号は 0 が先頭の行の上、行の数が最後の行の下。Shy で隠した行は数えず、`moveLayer` の位置は境界の隣に見えているレイヤーから求める | `HeaderDrag::boundary`、`Action::MoveLayer` の `boundary` |
| 入る位置（見出しのドラッグの） | ドラッグを離すとレイヤーが入る行の境界。見えている行の境界だけで、見えていない行の間へはドラッグしたままホイールで縦にスクロールして動かす | `TimelineController::header_drop`、`PaintState::header_drop` |
| 入る位置の線 | 入る位置に、見出しの列と行の領域を横に貫いて引く太さ 2 論理ピクセルの線 | `paint/header.rs` の `drop_line`、`DROP_LINE` |
| 斜線（無効なレイヤーの見出しの） | 無効なレイヤーの見出しに重ねる、小さな正方形を右上がりに並べた模様（Resolve の dimmed） | `paint/header.rs` の `hatch` |
| Shy | After Effects のレイヤーの Shy。Shy のフラグを立てたレイヤーの行を Hide Shy Layers で Timeline から外す。表示の状態で、描画と書き出しは変わらず、取り消しのスタックに入らない | `Shy`（`view_state/shy.rs`）、`ShyCommand`、`Action::Shy` |
| Shy のフラグ | Shy Selected Layers で選んだレイヤーに立てるフラグ。`.mm/view.json` にコンポジションごとのレイヤーの ID の集まりで置く。フラグを立てた行は、見出しの名前を accent の色で描く | `Shy::layers`、`Item::shy` |
| Shy の行 | Shy のフラグを立てたレイヤーの行。Hide Shy Layers で Timeline から外す | `Shy::hides` |
| Shy の行の数（見出しの列の上の角の） | 隠していれば隠した数（「4 hidden」）、隠していなければフラグを立てた行の数（「2 shy」。名前と同じ accent の色） | `header::shy_count`、`TimelineModel::hidden` |

## 置き換えた語

### UI の分かりやすさ（見出しのボタン）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 印（Shy の）、印を付ける・外す、印の付いた行 | Shy Selected Layers で選んだレイヤーに立てるもの。用語集の「印」はマーカーやフラグに置き換えた語 | Shy のフラグ、フラグを立てる・外す、フラグを立てた行（Shy の行） |
| 境目（行の） | 見出しのドラッグで入る、隣り合う行の間。用語集の「境目」は編集点と境界に置き換えた語で、見出しには行の境界の線もある | 行の境界 |
| `slot`、`slot_at`（見出しのドラッグの） | 行の境界の番号。slot は表示の状態の置き場所（`ViewSlot`）やフレームの置き場所（`FrameSlot`）と紛れる | `boundary`、`boundary_at` |
| 角（単体で） | 見出しの列の上の角。テストの名前と設計書で略していた | 見出しの列の上の角 |
| 取り消し（ドラッグの） | Esc で、離す前にドラッグを捨てること（`PointerEvent::Cancel`）。用語集の取り消しはトランザクションの undo | 取りやめ（ドラッグの） |
| 取り消しの列 | 表示の状態が入らない undo の並び。用語集の「列」（取り消しの）はスタックに置き換えた語 | 取り消しのスタック |
| 当てる（Shy を Timeline のモデルに） | フラグと隠す行をモデルに持たせること。用語集の「当てる」は適用に置き換えた語 | 反映する |
| 段ごと（`--play-test` の） | 手順ごとにスクリーンショットを残すこと。用語集の「段」は工程とステップに置き換えた語 | 手順ごと |
| 工程（見出しの / シーケンスを編集する） | Timeline の編集の工程の中の手順。用語集の工程（再生テストの）は再生、入力、Inspector、Timeline の編集の単位 | 手順 |
| 飛ばす（シーケンスを編集する手順を） | sequence の無いプロジェクトで行わないこと。用語集の「飛ばす」はスキップに置き換えた語 | スキップする |
| 引く（見出しの位置を / 見出しを `to` まで） | 位置を計算で出すこと / 見出しを動かすこと | 求める / ドラッグする |
