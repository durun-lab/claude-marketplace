## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| スナップの目標 | ドラッグ中の端を吸い付ける時刻と、その種類（再生位置、レイヤーの端、カットの端、キーフレーム、マーカー、コンポジションの始まりと終わり）。同じ近さなら、この順の先のものを採る。近さの差が 1 マイクロ秒以内なら同じ近さとみなす。動かしている端そのものは目標から外す | `SnapTarget`、`SnapKind`、`snap::targets`、`TIE_SECS` |
| 吸い付く距離 | 吸い付く目標までの距離の上限。8 論理ピクセルで、時間でなく画面の距離なので、ズームの倍率に依らない | `SNAP_PX` |
| 吸い付いた先の文 | 何に吸い付いたかを出す 1 行（Snapped to Playhead、Snapped to Layer Edge、Snapped to Clip Edge、Snapped to Keyframe、Snapped to Marker、Snapped to Start、Snapped to End）。ステータスバーが出す。カットは画面の文言の Clip と呼ぶ | `SnapTarget::message` |
| 反転（スナップの） | ドラッグ中にスナップの入/切のキー（N）を押している間だけ、入/切を逆にすること。離すかドラッグが終わると戻り、入/切そのものは変えない（FCP の N と同じ） | `TimelineCommand::ToggleSnapping`、`PointerEvent::SnapKeyUp`、`keys::release` |
| ポインターの入力のキュー | サーフェスのポインターの入力と、ドラッグに効くキー（スナップの入/切とその反転を戻す知らせ）を描画スレッドへ渡すキュー。同じキューで渡すので、ドラッグの終わりとの前後が保たれる | `Shared::input`、`PointerEvent` |
| スナップの表示 | 吸い付いたときだけ出す、ガイド線と、目盛りの帯の下端の下向きの三角形、行の領域の下端の上向きの三角形。三角形は時刻の列に全部入るときだけ描く。端が目標に届かない（素材の端や 1 フレームの長さで止まった）ときは出さない | `PaintState::snap`、`paint/snap.rs` |
| ガイド線（スナップの） | 吸い付いた時刻に出す、太さ 2 で時刻の列の全高の縦の線。吸い付いていないドラッグには出さない。用語集の識別子 `PaintState::guide` は `PaintState::snap` になった | `PaintState::snap`、`paint/snap.rs` の `GUIDE_W` |
| 端の強調（スナップの） | 吸い付いた先が帯の端のとき、その端のつまみの幅を帯の内側にガイド線の色で塗ること。レイヤーの帯の外に隠れたカットの端は塗らない | `paint/snap.rs` の `edge_highlight`、`EDGE_ALPHA` |
| ドラッグ中のツールチップ | 編集のドラッグの間、ポインターの右（入らなければ左）に出す、動かした量の矩形。量はポインターの向き（右が正）で、1 秒より短ければフレーム数（+12f）、1 秒からは時:分:秒:フレーム（−00:00:01:05）。矩形に重なるほかの文字は矩形の手前で切り、再生位置の線とスナップの表示は矩形の外だけに描く | `DragTooltip`、`PaintState::drag_tooltip`、`drag_tooltip`、`drag_amount`、`clear_tooltip_area` |

## 置き換えた語

### UI の分かりやすさ（Z2、スナップ）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 列（ポインターの入力と同じ列） | スナップの入/切のキーを描画スレッドへ渡す所。用語集で「列」はキューに置き換えた語 | ポインターの入力のキュー（`Shared::input`） |
| 印（人が置いた） | 同じ近さの目標を採る順の理由。用語集で「印」はマーカーとフラグに置き換えた語で、コンポジションのマーカーと紛れる | 人が置いた時刻 |
| 頭（字の） | ツールチップに重なる文字を外す条件。用語集で「頭」は先頭と接頭辞に置き換えた語 | 文字の左端（`Label::x`） |
| 列の端 | 三角形を描かない条件。「列」だけでは何の列か分からない | 時刻の列の端 |
| 入り、出、入つまみ、出つまみ、入/出つまみ | カットとレイヤーの端と、そのつまみ。用語集で「出（カットの）」はアウト点に置き換えた語 | イン点、アウト点、イン点のつまみ、アウト点のつまみ |
| フレームの境目に丸める | 吸い付かないときの端の時刻のそろえ方。K1 の置き換えと同じ言い方にする | いちばん近いフレームの先頭にそろえる |
| 既定の処理（キーを離したときの） | サーフェスが `WM_KEYUP` を渡す先 | 既定のウィンドウプロシージャ（`DefWindowProcW`） |
| 引く（つまみを）、引く（目標を） | この差分の新しい行で、ドラッグの意味と求める意味に同じ語を使っていた。「名前で引く」（表から探す）とも紛れる | ドラッグする、求める（前からある行は次のクリーンアップで置き換える） |
| 形のある図形、`Canvas::push_shape` | 切らずに描くひし形と三角形。矩形も形（`QuadShape::Rect`）の 1 つで、K1 の `push_quad` と同じ問題を別の決め方で解いていた | 矩形でない形（Quad の）。描く側で時刻の列に全部入るかを確かめて `Canvas::push_quad` で描く |
| `SnapKind::ClipEdge`、`clip`（試験の） | カットの端の目標。用語集で Clip は画面の文言だけの語で、コードと設計書はカット | `SnapKind::CutEdge`、`cut_edge`（画面の文は Snapped to Clip Edge のまま） |
| `MARK_W`、`MARK_H`、`marks`、`mark`、`top_mark`、`bottom_mark` | スナップの表示の三角形。帯のマーカー（`keymarks`、`MARK_PX`）とコンポジションのマーカーと紛れる | `TRIANGLE_W`、`TRIANGLE_H`、`triangles`、`triangle`、`top_triangle`、`bottom_triangle` |
| `DragTip`、`drag_tip`（モジュールとメソッド）、`PaintState::drag`、`tip_rect`、`tip_area`、`tip_fill`、`tip`（変数） | ドラッグ中のツールチップ。tip は先端とも読め、`PaintState::drag` はドラッグの状態（`Drag`）と紛れる | `DragTooltip`、`drag_tooltip`、`PaintState::drag_tooltip`、`tooltip_rect`、`tooltip_area`、`tooltip_fill`、`tooltip`（変数） |
| `clear`（文字を切る関数） | ツールチップに重なる文字を矩形の手前で切る。何を消すのかが名前に無く、`clear_tooltip_area` と紛れる | `clip_labels` |
| `snap::edge`（描く関数） | 吸い付いた帯の端を塗る関数。edge だけでは何を描くかが分からない | `snap::edge_highlight` |
| `lit`（試験の） | 端の強調の色。K1 は同じ語をホバーの意味で使っていて hovered に置き換えた（意味が定まらない） | `highlight` |
| `shown`（`snap::edges` の） | 動かしているレイヤーでないか。何が見えているのかと読める | `other_layer` |
