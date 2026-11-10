## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| Timeline の割り付け | Timeline の矩形を、ツールバー、目盛りの帯、見出しの列、行の領域、縦のスクロールバー、ズームバー、ステータスバーの矩形に分けたもの。まだ無い領域は大きさ 0 で、場所だけを決めてある | `TimelineLayout` |
| Timeline の高さの下限 | どの領域も潰さずに、Medium の行が 2 本見える Timeline の高さ。領域の大きさの定数から求める | `MIN_TIMELINE_H` |
| 行の領域 | Timeline の割り付けの、レイヤーの行を並べて縦にスクロールする矩形。時刻の x はこの左端から測る | `TimelineLayout::row_area` |
| 時刻の列 | 目盛りの帯と行の領域を合わせた列。再生位置の線とスナップのガイド線を引く範囲 | `TimelineLayout::time_column` |
| 行の並び | レイヤーの行ごとの高さと上端。縦の位置から行を二分探索で引く | `Rows` |
| 見えている所（Timeline の） | Timeline の割り付け、行の並び、ズームと横の位置、縦のスクロールの組。当たり判定、描画、文字、`--play-test` のクリックの位置が同じ値から計算する | `Viewport` |
| クリックの位置の計算（`--play-test` の） | サーフェスの割り付け、Timeline の見えている所、表示の倍率から、`--play-test` がクリックとドラッグを投げるクライアントの物理ピクセルを求めるもの。表示の状態は描画スレッドが置いたものを読む | `verify_place::Placement` |
| 縦のスクロール | 行の領域の上端に見えている所の、行の並びの上端からの距離（論理ピクセル） | `scroll_y` |
| 縦のスクロールバー | 行の領域の右の、縦のスクロールの位置を示すバー。中身が行の領域に入らないときだけ出す | `TimelineLayout::scrollbar` |
| スクロールボックス / 溝（スクロールバーの） | 縦のスクロールバーの中の、掴んで動かす矩形（Win32 の scroll box。thumb とも呼ぶ）。長さは見えている割合で、下限は 24 論理ピクセル / スクロールボックスの外の所。押すと押した側へ 1 画面ぶん送る | `scroll::thumb`、`scroll::scroll_for_thumb`、`Hit::Scrollbar` |
| 重ね順（Timeline の矩形の） | 矩形を描く順の区分（背景と縞、帯、帯に重ねるもの、帯のマーカー、暗がり、行の領域を囲む部品）。領域の描く関数は、呼ぶ順に関わらずこの区分に矩形を入れる | `Depth`（`Back`、`Bar`、`OnBar`、`BarMark`、`Shade`、`Surround`）、`Canvas` |
| 描く関数の入力（Timeline の） | 領域の描く関数と、公開する `timeline_quads`、`timeline_overlay` が受けるもの。Timeline のモデル、見えている所、描く時点の状態、帯の波形のピーク。描くものを足す PR は公開する関数を増やさずにフィールドを足す | `PaintInput`、`PaintInput::new` |
| 名前を置く所（帯の） | 帯の、名前を置く部分。Medium は上の 16 論理ピクセル（下端の 9 は帯のマーカー）、Large は上の 20 論理ピクセル（下は帯のマーカー、波形、フィルムストリップ）、Small は帯の全体 | `NAME_AREA_H`、`name_area` |
| 縮小の下限（Timeline の） | Ctrl+ホイールとズームのスライダーで縮められる下限の、秒あたりの論理ピクセル。全体表示の倍率と `MIN_PX_PER_SEC` の小さい方で、長いコンポジションも全体が収まるまで縮められる。全体表示そのものは `MIN_FIT_PX_PER_SEC`（目盛りの 1 時間の間隔でも時刻が重ならない）まで縮める | `TimeView::zoom_floor`、`MIN_FIT_PX_PER_SEC` |
| 開いたときの表示 | 表示の状態が無いときの Timeline の最初の表示。全体表示にし、sequence の行（無ければいちばん下の行）が見える所までスクロールする。表示の状態があればそれを戻す | `open_view`、`Ui::restore_view` |

## 置き換えた語

### UI の分かりやすさ（レイヤーの行と縦のスクロール）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 層（描く順の） | Timeline の矩形を重ねる順。用語集の「層」（依存の方向の区分）と、レイヤーと紛れる | 重ね順 |
| ビューポート | Timeline の割り付け、行の並び、ズームとスクロールの組 | 見えている所（Timeline の） |
| トラックの領域、行の欄 | レイヤーの行を並べて縦にスクロールする矩形。トラックは Mixer のトラックと紛れ、欄はキーフレームの欄と紛れる | 行の領域 |
| `TimelineLayout::rows` | 行の領域の矩形。行の並び（`Rows`、`Viewport::rows`）と同じ名前で、`vp.layout.rows` と `vp.rows` が紛れる | 行の領域（`TimelineLayout::row_area`） |
| つまみ（スクロールバーの）、`knob` | 縦のスクロールバーの掴んで動かす矩形。「つまみ」は帯の端のつまみと Mixer のつまみにも使い、`knob` は `thumb` と同じものの別の名前 | スクロールボックス（`thumb`） |
| `mm_ui_layout::thumb`、`reveal`、`clamp_scroll`、`max_scroll`、`scroll_for_thumb`、`MIN_THUMB_PX` | 縦のスクロールの関数を crate の根に出した名前。根の `thumb` はサムネイル（T5 の `thumbs.rs`）と紛れ、`reveal` は何を見せるかが名前に無い | `mm_ui_layout::scroll::thumb` など（モジュールを付けて呼ぶ） |
| `Frame`（mm-ui-layout の試験の） | 試験の見えている所の一式。動画のフレームと紛れる | `TestViewport` |
| `Scene`（描く関数の） | 領域の描く関数が見るもの。`mm perf run` の `Scene`（シナリオ）と紛れる | 描く関数の入力（`PaintInput`） |
| `Depth::Over` | 帯に重ねるもの（端のつまみ、外枠）の重ね順。文字の上に重ねる `timeline_overlay` と紛れる | 帯に重ねるもの（`Depth::OnBar`） |
| `Depth::Chrome`、行の領域の外の部品 | 目盛りの帯とスクロールバーの重ね順。Chrome のトレース（`mm_trace::chrome`）と WebView2 の Chromium と紛れる | 行の領域を囲む部品（`Depth::Surround`） |
| `NAME_BAND`、`name_band` | Large の帯の、名前を置く上の部分。英語の band が「帯」と読め、帯そのものと紛れる | 名前を置く所（`NAME_AREA_H`、`name_area`） |
| `grab`（スクロールボックスのドラッグの） | 押した点の、スクロールボックスの上端からの距離。何の量かが名前に無い | `grab_offset` |
