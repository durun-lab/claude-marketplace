## 再生と Timeline

| 用語 | 定義 | 識別子 |
|---|---|---|
| 区画（Inspector の） | Inspector で見出しの下にパラメータの行をまとめたもの（Layer、Transform、Crop、Source、Audio、Effect、Mask） | `Section`、`.mm-insp-section` |
| キーフレームの欄 | Inspector のパラメータの行に置く、選んだレイヤーの範囲（イン点からアウト点。コンポジションの時刻）の時間の表示。再生位置の線とキーフレームのひし形を出し、ひし形の無い所を押すと再生位置を動かす。Inspector の幅が 340px 以上なら行の右に置き（Premiere の Effect Controls の右）、狭いときは行の下に置いてキーフレームの無い行には出さない。再生位置が届くまでは出さない | `KeyframeStrip`、`.mm-insp-strip` |
| 区画のキーフレームの欄 | 区画の見出しに置くキーフレームの欄。まとめのひし形を出し、操作は受けない | `KeyframeSummary`、`.mm-insp-strip-summary` |
| ひし形（キーフレームの欄の） | キーフレームの欄で 1 つのキーフレームを表す回した正方形。押すと選び（accent の色）、ドラッグで `moveKeyframe`、Delete で `removeKeyframe`。行の名前の横のマーカー（キーフレームの）とは別 | `KeyMarks`、`.mm-insp-strip-key` |
| まとめのひし形 | 区画のキーフレームの欄に、区画の中のパラメータのキーフレームを時刻ごとに 1 つにして出すひし形（Premiere の summary keyframes） | `KeyframeSummary`、`data-summary-key` |

## 置き換えた語

### UI の分かりやすさ（K3、Inspector のキーフレームの欄）

| 元の語 | 文脈 | 正式な用語 |
|---|---|---|
| 時間の欄、欄（単体） | パラメータの行の時間の表示 | キーフレームの欄 |
| 組の見出しの欄、まとめの欄 | 区画の見出しの時間の表示 | 区画のキーフレームの欄 |
| まとめたひし形、見るだけのひし形 | 区画の見出しに時刻ごとに出すひし形 | まとめのひし形 |
| 組 | Inspector の `Section` | 区画 |
| 音量の区画 | gain と pan を持つ `audio` の区画 | 音の区画 |
| track、目盛り（`.mm-insp-strip-track`） | キーフレームの欄の内側の、時刻と x の対応を取る範囲。Timeline のトラックや段と紛れる | 時刻と x の対応を取る範囲（`.mm-insp-strip-axis`） |
| `.mm-insp-strip-head` | 再生位置の線 | `.mm-insp-strip-playhead` |
| コンテナー | `container-type` を付けた `.mm-insp`。MP4 や MOV のコンテナと紛れる | Inspector の幅 |
| 段（行の下の段） | 狭い Inspector でキーフレームの欄を置く所。Timeline の段と紛れる | 行の下 |
| フレームにそろえる | `frameTime` で時刻を丸めること | いちばん近いフレームの先頭にそろえる |
| `data-dragging` | ドラッグ中と、離して操作の結果を待つ間のひし形 | `data-moving` |
| `tip`（`SeekButton`） | ボタンの `title` に出すツールチップ | `title` |
