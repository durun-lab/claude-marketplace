# RFC hdr-input-tonemap: タイムラインの作業の明るさと、HDR の素材の入力の側のトーンマップ

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

- `colorConfig.workingRange`（JSON は `color_config.working_range`）は `sdr`（既定）か `hdr`。既定の値は保存しない。既存のプロジェクトは移行せず `sdr` になる。
- `sdr`: 素材の色空間が HDR（`bt2100_pq`、`bt2100_hlg`）のレイヤーの画像に、効果と合成の前で BT.2390 の EETF を掛けて SDR の白に収める。上限の明るさは素材の色空間だけで決める（1000 cd/m²。RFC 0180 と同じ）。合成の後のトーンマップは掛けない（`tone_map_peak` が 1）。
- `hdr`: 素材は 1 を超える値のまま合成し、SDR の出力は合成の後で収める。RFC 0180 の振る舞いのまま。
- 掛ける位置はレンダラーの `layer_item`（シーケンスのカットも通る）。素材の画像を GPU で straight に戻して掛け、premultiplied に戻す。全部の出力（プレビュー、SDR の書き出し、`GET /frame` と `mm frame` の PNG、配信の区間、提案の短い動画、モバイルの静止画）が同じレンダラーを通るので、同じ画素になる。
- フレームのキャッシュ: レイヤーの画像はトーンマップの前を覚え、入れ子の画像は後を覚える。入力の側のトーンマップが掛かるプロジェクトだけ、キーの色の指紋に印（`+input_tonemap:bt2390`）を足す。掛からないプロジェクトのキーは変えない。
- 配信の区間のキー: 今の `tone_map_peak` の値と範囲に掛かる素材の JSON（色空間を含む）で決まるので、形は変えない。`sdr` では置いていない HDR の素材でキーが変わらなくなる。
- モバイルの静止画（`mm-stream` の `render_webp`）も `tone_map_peak` で合成の後のトーンマップを掛ける（`hdr` のときだけ効く）。

## 口の変更

`mm-render-api`（意味の変更 1 つと追加 2 つ）:

| 変更 | 内容 |
|---|---|
| `tone_map_peak(project)` の意味 | 作業の明るさが `sdr` なら 1（合成の後に掛けない）。`hdr` なら今と同じ（素材に HDR があれば 1000 cd/m²）。形は同じで、呼ぶ所（プレビュー、書き出し、PNG、配信の区間のキー）は変えない |
| 追加 `input_tone_map_peak(project, space) -> f32` | 入力の側のトーンマップに渡す素材の上限の明るさ。`sdr` で `space` が HDR なら 1 を超え、それ以外は 1 |
| 追加 `input_tone_mapped(project) -> bool` | どれかの素材に入力の側のトーンマップが掛かるか。フレームのキャッシュのキーの印に使う |

`mm-doc`（API クレートではない）: `Project.color_config: ColorConfig`（serde の既定つき）、`ColorConfig { working_range: WorkingRange }`、`WorkingRange::{Sdr, Hdr}`。保存はプロジェクトの先頭の行（`version_stamp`、`copy_stamp`）に載せる。

`mm-ops`: 操作 `setColorConfig { color_config }`。逆操作は前の値を置く同じ操作。`touches` は `colorConfig` の 1 つ。

`RenderRequest` は変えない（作業の明るさはプロジェクトのデータから読む）。

## 試験

- `mm-render` の `input_tone`: `sdr` で、SDR の白は書き出しで 235、HDR の素材のハイライト（線形 2.0）は `bt2390_to_sdr` の値で白の下に残る。`hdr` は合成の前に掛けず、白は 214（今と同じ）。HDR の素材が無いプロジェクトは `sdr` と `hdr` で画素も NV12 もビット単位で同じ。作業の明るさを変えると入れ子の画像を引き直す。
- `mm` の `hdr`（FFmpeg あり）: `hdr` の PQ の書き出しの往復は今と同じ値。`sdr` では PQ と HLG のフィクスチャの帯が合成の前に白以下に収まる。
- `mm-render-api`、`mm-doc`、`mm-store`、`mm-stream` の単体の試験。

## 残したこと

- `sdr` では HDR の素材の中の基準の白（203 cd/m²）も 0.78 に下がり、SDR の素材の白より暗く見える。Premiere Pro の自動のトーンマップと同じ引き換え。合わせたいときは素材のレイヤーに色の効果を掛ける。
- `sdr` のプロジェクトを HDR の信号（`hevc_pq`、`hevc_hlg`）で書き出すと、HDR の素材のハイライトは SDR の白の下に収まったまま。HDR で納めるプロジェクトは `hdr` にする。
- 作業の明るさを切り替える画面は足さない。`setColorConfig` を `mm` と MCP から呼ぶ。
- 素材ごとの上限の明るさ（MaxCLL、マスタリングの表示）を読む、素材ごとにトーンマップを切る設定は足さない。
