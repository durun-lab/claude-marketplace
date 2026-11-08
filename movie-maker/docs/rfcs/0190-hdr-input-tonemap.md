# RFC 0190 hdr-input-tonemap: タイムラインの作業の明るさと、HDR の素材の入力の側のトーンマップ

状態: 提案

## 変える理由

HDR の素材があるプロジェクトを SDR で書き出すと、SDR の文字や画像の白（線形 1.0）が 0.78（8 bit の tv レンジで 235 → 214）に下がる（[RFC 0180](0180-hdr-sdr-match.md) の残り）。合成の後に全体へ掛けるトーンマップでは、SDR の白を 235 に置いたまま白より明るいハイライトを残せない。白の上には super-white しか無く、ファイルの再生では切られる。

トーンマップを HDR の素材の入力の側へ移し、SDR の素材と文字は通さない。そのために、タイムラインの作業の明るさ（SDR か HDR か）をプロジェクトの設定に足す。

## 先行事例

| 先行事例 | 採ったもの |
|---|---|
| Premiere Pro の「メディアを自動でトーンマップ」（SDR のシーケンスに置いた HDR のクリップだけに掛かり、SDR のクリップとグラフィックは変わらない） | 作業の明るさが SDR なら、HDR の素材のレイヤーだけに効果と合成の前でトーンマップを掛ける |
| DaVinci Resolve のタイムラインの色空間と入力のトーンマップ（SDR のタイムラインでは HDR の素材を入力の側で収める。HDR のタイムラインは出力の側で収める） | 作業の明るさ `sdr \| hdr` をプロジェクトに 1 つ持つ。`hdr` は今の HDR の書き出しの意味（合成の後に出力の側で収める）のまま |
| ITU-R BT.2408-7 の 7.2（SDR を損なえない制作では、HDR のカメラを down-mapper を通した SDR のモニターで調整する）、表 8 と 9（SDR のグラフィックと完成の SDR の変換の組）、7.3.4（down-mapping で HDR の基準の白を置く高さは、グラフィックを含む SDR の明るさとハイライトの残り方の兼ね合いで選ぶ） | SDR の白を 100% に保つ側を既定にする。HDR の素材の中の白とハイライトは down-mapping（BT.2390 の EETF）で白の下に収める |
| ITU-R BT.2408-7 の 7.3.4 と 7.4（super-white は切られうる。round-trip の損失を減らす down-mapper は super-white を使う） | super-white は使わない（RFC 0180 と同じ） |

## 新しく決めたこと

- `colorConfig.workingRange`（JSON は `color_config.working_range`）は `sdr`（既定）か `hdr`。既定の値は保存しない。
- 既存のプロジェクト: 名前つきの移行 `20261108_working_range` が、HDR の素材があり HDR の信号（`hevc_pq`、`hevc_hlg`。`mm_doc::HDR_SIGNAL_CODECS`）で書き出すレンダーターゲットを持つプロジェクトだけを `hdr` にし、HDR の書き出しのハイライトを前のまま残す。ほかの既存のプロジェクトは `sdr` になり、SDR の書き出しの白が 214 から 235 に戻る（この RFC の目的）。新しいプロジェクトは `sdr`。
  - 先行事例: Premiere Pro は「メディアを自動でトーンマップ」を足したとき、新しいシーケンスでは入れ、前の版で作ったプロジェクトは切った状態で開いて前の見え方を保った（[Adobe のベータの告知](https://community.adobe.com/t5/premiere-pro-beta-discussions/discuss-tone-mapping-in-premiere-pro-beta/m-p/13127814/highlight/true)）。
  - 新しく決めたこと: 全部の既存のプロジェクトを `hdr` にはしない。SDR だけで書き出すプロジェクトは `hdr` のままだと白が 214 のまま残り、直したい不具合が残るため。ハイライトを失う（戻せない）のは HDR の信号の書き出しだけなので、そこだけ前の振る舞いを保つ。
  - 移行の後に HDR の信号のレンダーターゲットを足して `sdr` のまま書き出すと、`mm-export` が納品物の結果（`DeliveryReport.warnings`）に `setColorConfig` で `hdr` にするよう警告を出す。書き出しは止めない（SDR の白を揃えた HDR の納品もありうる）。
- `sdr`: 素材の色空間が HDR（`bt2100_pq`、`bt2100_hlg`）のレイヤーの画像に、効果と合成の前で BT.2390 の EETF を掛けて SDR の白に収める。上限の明るさは素材の色空間だけで決める（1000 cd/m²。RFC 0180 と同じ）。合成の後のトーンマップは掛けない（`tone_map_peak` が 1）。
  - 引き換え: HDR の素材のレイヤーの効果（ぼかし、色、光）、合成モード（加算、スクリーン）、トラックマットの輝度は、SDR に収めた後の値で計算する。白を超えるハイライトが加算やぼかしで周りに広がる量は `hdr` より小さい。Premiere Pro の自動のトーンマップと Resolve の入力のトーンマップも、効果の前で収める同じ位置。HDR の値で効果を掛けたいプロジェクトは `hdr` にする。
- `hdr`: 素材は 1 を超える値のまま合成し、SDR の出力は合成の後で収める。RFC 0180 の振る舞いのまま。
- 掛ける位置はレンダラーの `layer_item`（シーケンスのカットも通る）。素材の画像を GPU で straight に戻して掛け、premultiplied に戻す。全部の出力（プレビュー、SDR の書き出し、`GET /frame` と `mm frame` の PNG、配信の区間、提案の短い動画、モバイルの静止画）が同じレンダラーを通るので、同じ画素になる。
- フレームのキャッシュ: レイヤーの画像はトーンマップの前を覚え、キーに作業の明るさを入れない（切り替えてもデコードし直さない）。入れ子の画像は後を覚え、入力の側のトーンマップが掛かるプロジェクトだけ、そのキーの設定の指紋に `+input_tonemap:bt2390` を足す。掛からないプロジェクトのキーは変えない。
- Source のビューアーは、素材だけを載せたプロジェクトに本体の作業の明るさを写す。Program と同じ合成の後のトーンマップ（`Ui::set_hdr_peak`）を受けるので、`hdr` の本体で HDR の素材に二重に掛けない。
- 配信の区間のキー: 今の `tone_map_peak` の値と範囲に掛かる素材の JSON（色空間を含む）で決まるので、形は変えない。`sdr` では置いていない HDR の素材でキーが変わらなくなる。
- モバイルの静止画（`mm-stream` の `render_webp`）も `tone_map_peak` で合成の後のトーンマップを掛ける（`hdr` のときだけ効く）。

## インターフェースの変更

`mm-render-api`（意味の変更 1 つと追加 2 つ）:

| 変更 | 内容 |
|---|---|
| `tone_map_peak(project)` の意味 | 作業の明るさが `sdr` なら 1（合成の後に掛けない）。`hdr` なら今と同じ（素材に HDR があれば 1000 cd/m²）。形は同じで、呼ぶ所（プレビュー、書き出し、PNG、配信の区間のキー）は変えない |
| 追加 `input_tone_map_peak(project, space) -> f32` | 入力の側のトーンマップに渡す素材の上限の明るさ。`sdr` で `space` が HDR なら 1 を超え、それ以外は 1 |
| 追加 `input_tone_mapped(project) -> bool` | どれかの素材に入力の側のトーンマップが掛かるか。入れ子の画像のキーに `+input_tonemap:bt2390` を足すかに使う |

`mm-doc`（API クレートではない）: `Project.color_config: ColorConfig`（serde の既定つき）、`ColorConfig { working_range: WorkingRange }`、`WorkingRange::{Sdr, Hdr}`、`HDR_SIGNAL_CODECS`、`hdr_signal_from_sdr_range(project, codec)`、移行 `20261108_working_range`。保存はプロジェクトの先頭の行（`version_stamp`、`copy_stamp`）に載せる。

`mm-ops`: 操作 `setColorConfig { color_config }`。逆操作は前の値を置く同じ操作。`touches` は `colorConfig` の 1 つ。

`RenderRequest` は変えない（作業の明るさはプロジェクトのデータから読む）。

## 試験

- `mm-render` の `input_tone`: `sdr` で、SDR の白は書き出しで 235、HDR の素材のハイライト（線形 2.0）は `bt2390_to_sdr` の値で白の下に残る。`hdr` は合成の前に掛けず、白は 214（今と同じ）。HDR の素材が無いプロジェクトは `sdr` と `hdr` で画素も NV12 もビット単位で同じ。作業の明るさを変えると入れ子の画像を引き直し、レイヤーの画像は覚えたものを使う。`sdr` の加算の合成は収めた後の素材を足す。
- `mm` の `hdr`（FFmpeg あり）: `hdr` の PQ の書き出しの往復は今と同じ値。`sdr` では PQ と HLG のフィクスチャの帯が合成の前に白以下に収まる。
- `mm-doc` の移行: HDR の素材と HDR の信号のレンダーターゲットを両方持つプロジェクトだけ `hdr` になり、済ませたプロジェクトと設定を持つプロジェクトは変えない。`mm-encode-ffmpeg`: `HDR_SIGNAL_CODECS` がコーデックの表の `OutputSignal::is_hdr` と同じ。`mm-export`: `sdr` のまま HDR の信号で書き出すときだけ警告する。
- `mm-render-api`、`mm-store`、`mm-stream`、アプリの Source の単体の試験。

## 残したこと

- `sdr` では HDR の素材の中の基準の白（203 cd/m²）も 0.78 に下がり、SDR の素材の白より暗く見える。Premiere Pro の自動のトーンマップと同じ引き換え。合わせたいときは素材のレイヤーに色の効果を掛ける。
- `sdr` のプロジェクトを HDR の信号（`hevc_pq`、`hevc_hlg`）で書き出すと、HDR の素材のハイライトは SDR の白の下に収まったまま（警告を出す）。HDR で納めるプロジェクトは `hdr` にする。
- 作業の明るさを切り替える画面は足さない。`setColorConfig` を `mm` と MCP から呼ぶ。Source で素材を開いたまま切り替えると、開き直すまで前の作業の明るさで映る。
- 素材ごとの上限の明るさ（MaxCLL、マスタリングの表示）を読む、素材ごとにトーンマップを切る設定は足さない。
