## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 映像の領域（Program と Source の） | ビューアーの、映像を縦横比を保って置く矩形。再生の操作のバーを含まない。映像の矩形（レターボックスの余白を除いた所）と Review の描き込みは、この領域から求める | `SurfaceLayout::program`、`SurfaceLayout::source` |
| ビューアーの領域 | サーフェスの Timeline より上の、Program（と Source）の映像の領域と再生の操作のバーを並べる所。バーの高さの 2 倍より低ければ、バーを出さずに映像の領域に回す | `split_viewers`、`MIN_VIDEO_H` |
| 再生の操作のバーの矩形 | 映像の領域の真下の、高さ 56 論理ピクセルの矩形。上からタイムコードの行、ジョグバー、ボタンの行 | `SurfaceLayout::program_transport`、`SurfaceLayout::source_transport`、`TRANSPORT_H` |
| タイムコードの行 | 再生の操作のバーの上端の行。左の欄に再生位置、右の欄に Program は全体の長さ、Source はイン点とアウト点のタイムコードを出す | `Cells::position`、`Cells::range` |
| 未設定のタイムコード | イン点やアウト点が無いときに、タイムコードの代わりに出す `--:--:--:--` | `NO_TIMECODE` |
| ジョグバーの溝 | ジョグバーの、左端が 0 秒、右端が全体の長さ（Source は素材の長さ）の横線。Program は再生位置まで、Source はイン点とアウト点の間を塗り、再生位置に線を引く。クリックはバーの全幅で受け、溝の外は端に寄せる | `Cells::track`、`jog_track` |
| ボタンの行 | 再生の操作のバーの下端の行。ボタンを中央に並べる | `place_buttons` |
| 名前のボタン / アイコンのボタン | アイコンの代わりに短い名前（In、Out、Clear、Insert、Overwrite）を枠の中に出すボタン / アイコンだけのボタン。ボタンの行に全部の名前が入らなければ、名前のボタンもアイコンにする | `ButtonSpec::name`、`ButtonCell::named` |
| 入（Play Forward の） | 順方向に再生している間の Play Forward の見た目。Mute と Solo の入と同じく、テーマの `accent` で塗る | `ViewerState::playing`、`ButtonLook::lit` |
| 取りやめ（ボタンの） | ボタンを押したまま外へずらして離し、実行しないこと（Windows のボタンと同じ）。外から戻って離せば実行する。押している間は、ポインターが上にあるときだけ押した見た目にする | `TransportController::pressed` |
| 再生の操作のバーの部品 | ジョグバーか、コマンドの表の ID を持つボタン。当たり判定と描画が、ポインターの下の部品を見る | `TransportPart`、`transport::part_at` |

## 置き換えた語

### UI の分かりやすさ（再生の操作のバー）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| バー（単独）、`program_bar`、`source_bar`、`bar`（`TransportSync` の変数） | 映像の領域の真下の、再生の操作のバー。「バー」だけではジョグバー、ズームバー、ステータスバー、メニューバーと紛れ、英語の bar は Timeline の帯（`Bar`）と紛れる | 再生の操作のバー（`program_transport`、`source_transport`、`transport`） |
| 映像の矩形（`SurfaceLayout::program` の） | 映像を置く所。用語集の映像の矩形はレターボックスの余白を除いた所で、置く所とは違う | 映像の領域 |
| 上の段 | Timeline より上の、映像の領域とバーを並べる所。「段」は 1 字で意味が定まらない | ビューアーの領域 |
| 時刻、時刻の欄、時刻の行、`time`、`side`、`TIME_H`、`NO_TIME` | バーの上端に出す `HH:MM:SS:FF` と、その左右の矩形。目盛りの時刻と紛れ、`side` は何の欄か分からない | タイムコード、タイムコードの行の左の欄と右の欄（`position`、`range`、`TIMECODE_ROW_H`、`NO_TIMECODE`） |
| 線、当たり（ジョグバーの） | ジョグバーの全体の長さを表す横線 / クリックを受ける矩形 | ジョグバーの溝（`track`） / ジョグバーのクリックを受ける矩形（`jog`） |
| つまみ（再生位置の）、`THUMB_W` | ジョグバーの再生位置の縦の線。帯の端のつまみと紛れ、掴んで動かすものではない | 再生位置の線（`PLAYHEAD_W`） |
| 進んだ分 | ジョグバーの左端から再生位置までの塗り | 再生位置までの塗り |
| 絵柄、絵 | ボタンの形 | アイコン |
| 棒、腕、軸 | アイコンの縦の矩形 / 括弧の上下の横の矩形 / 矢印の縦の矩形 | 縦線 / 括弧の横線 / 矢印の縦線 |
| 下地 | ポインターが乗ったボタンの塗り | 背景（ボタンの） |
| もち主（バーの） | バーを付ける Program か Source。用語集の持ち主はコマンドを実行する所（`Owner`） | バーを付けるビューアー（`Viewer`） |
| 部品、`Target`、`hit`、`under` | ポインターの下のジョグバーかボタン。`Target` はクレートの直下に出て、何の対象か分からない | 再生の操作のバーの部品（`TransportPart`、`part_in`、`part_at`） |
| `Slot`、置き場所 | 1 つのボタンの矩形と定義。アプリの slot（`FrameSlot`、`ViewSlot`）は共有の状態の置き場で紛れる | ボタンの矩形（`ButtonCell`） |
| `Face` | ボタンの見た目を決める状態（押せるか、ポインターが乗っているか、押しているか、入か） | ボタンの見た目（`ButtonLook`） |
| `ROW_H`（再生の操作のバーの） | ボタンの行の高さ。Timeline の行の高さと紛れる | `BUTTON_ROW_H` |
| 範囲を決めるボタン、Timeline に入れるボタン | Source のバーの Mark In、Mark Out、Clear In and Out / Insert、Overwrite | イン点とアウト点のボタン / Insert と Overwrite のボタン |
| 点（「点を付ける」「点が無ければ」） | Source の素材のイン点とアウト点 | イン点とアウト点 |
| `ClearMarks`、`clear_marks`、`marks_text`、`marks`（`TransportSync` の） | Clear In and Out の編集と、イン点とアウト点の文字と値。marks は帯のキーフレームのマーカー（`keymarks`）と紛れる | `ClearInOut`、`clear_in_out`、`in_out_text`、`in_out` |
| 破線 | イン点やアウト点が無いときの `--:--:--:--`。線ではなくハイフンの並び | 未設定のタイムコード |
| 覚え | 押せるかとイン点・アウト点を、通し番号が変わったときだけ計算し直すために持つ値 | 直前の計算の結果 |
| 片方のバー | Program か Source の 1 つのバー | 1 つのバー |
| 潰れる（ボタンが） | 幅が足りずにボタンの幅が縮み、アイコンが重なること | ボタンの幅が縮む |
| 段（`verify_transport.rs` の） | 再生の操作のバーの工程の中の 1 つの確認 | 手順 |
