## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 開く（キーフレームの行を） | キーフレームの開閉（見出しの）か `U`（Show Keyframe Editor）で、レイヤーの行の下にキーフレームの行を出すこと。`U` は選んだレイヤー（キーフレームの行が無ければ開かない）、選んでいなければキーフレームのある全部のレイヤーを開く（全部開いていれば全部閉じる）。開いたレイヤーの ID は表示の状態で、`.mm/view.json` に置く | `RowHeights::keyframe_rows_open`、`toggle_open`、`KeyframeAction::ToggleRows`、`timeline.show_keyframe_editor` |
| 行の見出し（キーフレームの行の） | キーフレームの行の、見出しの列の部分。パラメータの名前（「Transform › Position」）を出し、押すとそのパラメータを選ぶ | `header_rect`、`Hit::KeyframeRowHeader` |
| ひし形（キーフレームの行の） | キーフレームの行でキーフレーム 1 つを表す形。一辺 11 論理ピクセル。押すと選び、ドラッグで動かす。選んだものは accent の輪で囲む。行の領域に収まらない位置では縦線にして切る | `RowKeyframe`、`ROW_DIAMOND_PX`、`Hit::RowKeyframe`、`KeyframeIndex` |
| まとめる（キーフレームの行のひし形を） | 帯のマーカーをまとめる幅（6 論理ピクセル）で時刻を区切った列ごとに、ひし形を 1 つだけ描くこと。描くのは見えている時刻の範囲だけ。列は左端の時刻によらず時刻とズームで決まる。選んだものとポインターの乗ったものはまとめない | `merge_column`、`keyframes_within`、`MERGE_PX` |
| 選んだキーフレーム（キーフレームの行の） | キーフレームの行で選んだひし形。レイヤーの ID、行の名前、時刻で持ち、モデルが変わると探し直す。Delete（Remove Keyframe）で外す | `Picked`、`RowPick`、`timeline.remove_keyframe` |
| 選んだパラメータ（Timeline の） | キーフレームの行で選んだ行のパラメータ。パスと通し番号を `mm:layer-selected` の `param` で送り、Inspector と Graph で同じパラメータを選ぶ。Inspector は通し番号が変わったときだけ選び直す | `ParamSelection`、`KeyframeSlot`、`KeyframeAction::Select` |
| 範囲（キーフレームの行のパラメータの） | キーフレームを動かせる時間。レイヤーのパラメータはコンポジションの全体、再フレーミングはカットが映るコンポジションの範囲。ドラッグはその最後のフレームの始まりまで | `ParamSpan`、`RowKeyframe::min`、`RowKeyframe::max` |
| 押せる最初のコマンド（キーの） | 1 つのキーに割り当てた複数のコマンドのうち、表の順で最初に押せるもの。Delete はカットを選んでいれば Lift、キーフレームを選んでいれば Remove Keyframe | `keys::command_now` |

## 置き換えた語

### UI の分かりやすさ（K2、キーフレームの行）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| `RowKey`、`key`（キーフレームの行の添字の組） | ひし形 1 つの位置。Key はキーボードのキー（`keys.rs`、`[` と `]`、`U`）と紛れる（K1 の `Hit::Key` と同じ） | `KeyframeIndex`、`at` |
| `key_center`、`ROW_KEY_PX`、`press_key` | ひし形の中心、一辺、ひし形を押す試験の補助 | `keyframe_center`、`ROW_DIAMOND_PX`、`press_diamond` |
| `KeySource`、`keys`（行の元の） | 行のキーフレーム 1 つの元 | `KeyframeSource`、`keyframes` |
| Keyframe Editor（コードと設計書で） | 画面の文言の Show Keyframe Editor（Resolve の語）が開くもの。コードと設計書では行を指す | キーフレームの行（`keyframe_rows`） |
| 引く（前後のキーフレームを） | 再生位置の前後のキーフレームの時刻を計算で出すこと | 求める |
| `key`、`keys`（キーフレームの位置と並びの変数、試験でキーフレームを作る関数） | 上の 2 つの置き換えの残り。Key はキーボードのキーと紛れる | `at`、`keyframes`、`keyframe` |
| 行どうしの境の線 | キーフレームの行の上端の 1 px。「境の線」は見出しの段で置き換えた語 | 行の境界の線 |
| `band`（キーフレームの行の） | 行の背景の矩形。英語の band が「帯」と読める（見出しの `band` と同じ） | 行の背景（`back`） |
| `reach` | ひし形の中心から左右の端（当たり判定は余白を足した端）まで。帯のマーカーの段で置き換えた語 | `half` |
| `grab`（キーフレームのドラッグの） | 押した点の時刻。何の量かが名前に無い（スクロールボックスの `grab` と同じ） | `grab_time` |
| `follow`（`KeyframeSlot` の） | モデルを作り直した後の描画スレッドの選択に合わせること。何に従うのかが名前に無い | `match_pick` |
| 場面で使い分けるキー、`SCENE_KEYS` | 2 つのコマンドに割り当てたキー（Delete）。「場面」は用語集で置き換えた語 | 2 つのコマンドに割り当てたキー（`TWO_COMMAND_KEYS`） |
| `Target`（再生テストのキーフレームの行の） | 行を開いたレイヤー。何の対象か分からない（再生の操作のバーの `Target` と同じ） | `OpenedLayer` |
| `marks`、`keys`（再生テストの前後のキーフレームへの移動の） | `[` と `]` で止まる時刻。帯のマーカーとキーボードのキーと紛れる | `stops`、`times` |
| 飛ばす（再生テストの手順を） | 当てはまらない手順を ok でも ng でもなく記録すること。用語集の「飛ばす」はスキップするに置き換えた語 | スキップする（`Log::skip`） |
