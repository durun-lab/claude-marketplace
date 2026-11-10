## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| ツール（Timeline の） | 帯の中のドラッグとクリックが何になるかを選ぶ状態。Selection、Trim、Blade の 3 つで、キーは A、T、B、コマンドは Selection Tool、Trim Tool、Blade Tool（Resolve の Selection Mode、Trim Edit Mode、Blade Edit Mode と FCP の A、T、B と同じ）。表示の状態で、取り消しの履歴にも `.mm/view.json` にも入れず、開くたびに Selection から始める | `Tool`、`TimelineController::tool`、`Shared::tool`、`TimelineCommand::SelectionTool`、`TimelineCommand::TrimTool`、`TimelineCommand::BladeTool` |
| Selection（ツール） | レイヤーの帯の中のドラッグはレイヤーを時間の上で動かし（`shiftLayer`）、カットの帯の中は選ぶだけ。帯の端は Selection と Trim で同じ（カットは `ripple`、Alt で `roll`、レイヤーは `trimLayer`） | `Tool::Selection`、`gesture::shift_layer_edit` |
| Trim（ツール） | カットの帯の中のドラッグは `slip`、Shift で `slide`。レイヤーの帯の中は選ぶだけ | `Tool::Trim`、`gesture::body_edit` |
| Blade（ツール） | カットの上のクリック（端のつまみの上も）で、ポインターの時刻でカットを割る（`split`）。割る時刻がカットの端に丸まる所、隙間、レイヤーの上では何もしない | `Tool::Blade`、`Action::Split` |
| 割る時刻 | Blade のクリックでカットを割る時刻。ポインターの時刻を、Snapping が入なら再生位置に吸い付け（吸い付く距離は `SNAP_PX`）、いちばん近いフレームの先頭にそろえる | `TimelineController::blade_time` |
| 割る時刻の線 | Blade のときに、ポインターの下のカットに引く縦線。割る時刻に、帯の高さより少し長く、テーマの `text` の色で出し、再生位置に吸い付いたときだけガイド線の色にする（FCP のスキマーと同じ考え）。再生位置の線より上に描く | `BladeLine`、`BladeLine::snapped`、`PaintInput::blade_line`、`paint/blade.rs`、`Depth::Pointer` |
| スキマー（FCP の） | Final Cut Pro で、ポインターの位置を示す縦線。Blade のツールはスキマーの所で割る | なし（先例の語） |
| Split Clip | 再生位置にあるカットを割るコマンド。キーは Ctrl+B（B は Blade Tool） | `clip.split`、`EditAction::Split` |
| レイヤーを時間の上で動かすドラッグ | Selection でレイヤーの帯の中を引くドラッグ。ドラッグ中は帯とキーフレームのマーカーを動かして描き、離したときに 1 つの `shiftLayer` にする | `Drag::ShiftLayer`、`shift_layer_drag`、`keymarks::marks_shifted` |

## 置き換えた語

### Timeline のツール（Z4）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| `Drag::Move`、`move_drag`、`gesture::layer_move` | Selection でレイヤーの帯を時間の上で動かすドラッグ（`shiftLayer`）。move は、見出しの縦のドラッグで重なりの順を変える `moveLayer`（`Action::MoveLayer`）と紛れる | `Drag::ShiftLayer`、`shift_layer_drag`、`gesture::shift_layer_edit` |
| `move_layer`、`undo_move`、`21-move-dragging`、`22-move-layer`、`move_steps`、`MOVE_SECS`、`MOVE_TOLERANCE` | `--play-test` の工程と画像の名前、関数、定数。同じく `moveLayer` と紛れる | `shift_layer`、`undo_shift_layer`、`21-shift-layer-dragging`、`22-shift-layer`、`shift_layer_steps`、`SHIFT_SECS`、`SHIFT_TOLERANCE` |
| `marks_moved` | ドラッグ中の `shiftLayer` で動かした後に見えるマーカー | `marks_shifted` |
| `blade`（`TimelineController` と `PaintInput` のフィールド）、`update_blade` | 割る時刻の線。blade だけではツール（`Tool::Blade`）と紛れる | `blade_line`、`update_blade_line` |
| `select`（`verify_edit/tools.rs`） | ツールのキーを押し、描画スレッドが置くまで待つ関数。レイヤーの選択（`Ui::select`）と紛れる | `select_tool` |
| `targets_visible`、押す帯 | ツールの通しでクリックする行が見えているか。target はスナップの目標と紛れ、押す帯は何の帯かが分からない | `tool_rows_visible`、クリックする行 |
| skimmer、skimmer の線 | FCP の先例（コメント、設計書、用語集） | スキマー（FCP の） |
| 試験、自動試験 | ツールのテストの説明、ホバーの画面を撮る関数。用語集で試験はテストに置き換えた語 | テスト、`--play-test` |
