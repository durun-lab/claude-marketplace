## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| グループ（ツールバーの） | ツールバーのボタンのまとまり。左から取り消し（Undo、Redo）、ツール（Selection、Trim、Blade）、編集（Split Clip、Lift、Extract）、スナップ（Snapping）、ズーム（Zoom Out、スライダー、Zoom In、Full Extent Zoom）で、間に区切りの縦線を引く。ズームのグループだけは右端に寄せる | `toolbar::Group`、`toolbar::BUTTONS` |
| 名前の幅（ツールバーの） | ボタンの名前を、地の文の字の大きさで文字の仕組みが測った幅。当たり判定、矩形、文字、`--play-test` のクリックの位置が同じ値を見る | `ToolbarNames`、`Shared::toolbar_names` |
| 名前を省く順（ツールバーの） | 全部の名前が入らないときに、名前を省くグループの順。取り消し、ズーム、ツール、スナップ、編集（Windows のリボンの ScalingPolicy と同じく、グループごとに小さい形にする） | `NAME_DROP_ORDER` |
| 記号のフォント | Windows のアイコンの字のフォント。Segoe Fluent Icons（Windows 11）、無ければ Segoe MDL2 Assets（Windows 10）。2 つは同じ符号位置に同じ意味の字を割り当てている | `SYMBOL_FONTS`（`mm-ui-wgpu` の `fonts.rs`） |
| 私用領域（Unicode の） | U+E000 から U+F8FF。符号位置の意味をフォントが決めるので、この範囲の字はフォールバックに任せず、記号のフォントを指してシェーピングする | `SYMBOL_RANGE`、`is_symbol` |
| 字のアイコン / 標準の記号の字 | 記号のフォントの字 1 つで描くアイコン（Undo、Redo、Split Clip、Zoom Out、Zoom In、Full Extent Zoom）/ 記号のフォントが無いときに代わりに描く、標準のフォントにある同じ意味の字（↶、↷、✄、−、+、⤢） | `Icon::Glyph` の `symbol` と `plain`、`ToolbarState::symbols` |
| 入の見た目（ツールバーの） | Snapping が入のときと、今のツール（Selection）のボタンの見た目。テーマの `accent` で塗る | `ButtonLook::lit` |
| 押した見た目（ボタンの） | ボタンを押している間の見た目。背景を濃くし、アイコンと名前を右下へずらす。押したまま外へずらすと外す | `ButtonLook::place`、`PRESS_SHIFT` |
| スライダー（ツールバーの） / スライダーの溝 / スライダーのつまみ | 秒あたりの論理ピクセルを対数で動かす横長の部品 / つまみの中心が動く横線。左端が縮小の下限、右端が `MAX_PX_PER_SEC` / 今の倍率の位置の矩形。掴んで動かす | `toolbar::Slider`、`zoom_fraction`、`zoom_px`、`THUMB_W` |

## 置き換えた語

### UI の分かりやすさ（Timeline のツールバー）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 押された見た目（Snapping の） | Snapping が入のときの見た目。押している間の「押した見た目」と紛れる | 入の見た目 |
| まとまり（ツールバーのボタンの） | 用語集の「組」の行で、ツールバーのボタンのまとまりはグループにそろえてある | グループ（ツールバーの） |
| つまみ、溝（単体。スライダーの） | 用語集の「つまみ」は帯の端の掴む所で、「溝」はズームバー、スクロールバー、ジョグバーにもある | スライダーのつまみ、スライダーの溝 |
