## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| ツール（Timeline の） | 帯の中のドラッグとクリックが何になるかを選ぶ状態。Selection、Trim、Blade の 3 つで、キーは A、T、B、コマンドは Selection Tool、Trim Tool、Blade Tool（Resolve の Selection Mode、Trim Edit Mode、Blade Edit Mode と FCP の A、T、B と同じ）。表示の状態で、取り消しの履歴にも `.mm/view.json` にも入れず、開くたびに Selection から始める | `Tool`、`TimelineController::tool`、`Shared::tool`、`TimelineCommand::SelectionTool`、`TimelineCommand::TrimTool`、`TimelineCommand::BladeTool` |
| Selection（ツール） | レイヤーの帯の中のドラッグはレイヤーを時間の上で動かし（`shiftLayer`）、カットの帯の中は選ぶだけ。帯の端は全部のツールで同じ（カットは `ripple`、Alt で `roll`、レイヤーは `trimLayer`） | `Tool::Selection`、`gesture::layer_move` |
| Trim（ツール） | カットの帯の中のドラッグは `slip`、Shift で `slide`。レイヤーの帯の中は選ぶだけ | `Tool::Trim`、`gesture::body_edit` |
| Blade（ツール） | カットの上のクリックで、ポインターの時刻でカットを割る（`split`）。レイヤーと隙間の上では何もしない | `Tool::Blade`、`Action::Split` |
| 割る時刻 | Blade のクリックでカットを割る時刻。ポインターの時刻を、Snapping が入なら再生位置に吸い付け（吸い付く距離は `SNAP_PX`）、いちばん近いフレームの先頭にそろえる | `TimelineController::blade_time` |
| 割る時刻の線 | Blade のときに、ポインターの下のカットに引く縦線。割る時刻に、帯の高さより少し長く、ガイド線の色で出す（FCP の Blade の skimmer の線と同じ考え） | `BladeLine`、`PaintInput::blade`、`paint/blade.rs` |
| Split Clip | 再生位置にあるカットを割るコマンド。キーは Ctrl+B（B は Blade Tool） | `clip.split`、`EditAction::Split` |
