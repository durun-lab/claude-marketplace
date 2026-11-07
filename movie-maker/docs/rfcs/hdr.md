# RFC: HDR の素材を読み、HDR のまま合成し、HDR で書き出す（段階 8）

状態: 提案

## 変える理由

- デコーダーは PQ と HLG の素材を `Unsupported` で断っていた。10 bit の SDR の素材も CPU の経路では 8 bit の RGBA を経由し、階調を落としていた（`15-roadmap.md` の「後の段階の注意」の 1 デコード）。
- 線形の光で 1 を超える値が 3D LUT で切られていた（`color.lut`、OCIO の変換を 3D LUT にベイクしたもの。同じく 8 HDR）。
- 書き出しは Rec.709 の SDR だけで、HDR10 と HLG を書けなかった。

先行事例:

| 事例 | 採ったもの |
|---|---|
| ITU-R BT.2408（HDR の運用） | SDR の白を HDR の基準の白 203 cd/m² に置く。作業色空間の線形の 1.0 を 203 cd/m² にし、SDR の文字や画像を HDR の素材に重ねても白が揃う |
| ITU-R BT.2100、SMPTE ST 2084、ARIB STD-B67 | PQ と HLG の式（HLG の OOTF はシステムガンマ 1.2、表示の上限 1000 cd/m²） |
| ITU-R BT.2390 の EETF | プレビューで HDR を SDR の表示に収めるトーンマップ。素材の上限が表示の上限と同じなら恒等 |
| OCIO の `ociobakelut --shaperspace`、Resolve の 1D シェーパーつきの `.cube` | 3D LUT の前に 1D の曲線を置き、線形の光を格子に収める |
| x265 の `hdr10`、`master-display`、`max-cll` | HDR10 のマスタリングの表示とコンテンツの明るさを SEI に書く |

新しく決めたこと（先例が解かない所）:

- PQ と HLG の伝達関数は、ほかの式と同じく `mm_types::transfer` に CPU と WGSL を 1 か所で置き、デコーダー、色の変換、書き出し、プレビューが同じ式を使う。
- HLG の OOTF は輝度ではなくチャンネルごとに掛ける。伝達関数をチャンネルごとの式に保ち（`mm-color-lut` の空間の作り）、無彩色では BT.2100 の式と一致する。
- `color.lut`（`.cube`）は定義域の外へ出た分を、LUT の端の値に足して持ち越す。SDR 向けの LUT を HDR の素材に掛けても、ハイライトの階調が白に潰れない。

## 決めたこと

### 色空間の名前（足すだけ）

| 名前 | 伝達関数 | 原色 |
|---|---|---|
| `bt2100_pq` | PQ。線形の 1.0 が 203 cd/m² | Rec.2020 |
| `bt2100_hlg` | HLG。表示の上限 1000 cd/m² の OOTF を含む | Rec.2020 |

符号化値 (1,1,1) が白にならないので `UNIT_WHITE_SPACES` に入れない。デコーダーと取り込み（ffprobe）は PQ と HLG の素材をこの名前で名乗る。OCIO の組み込みの設定の PQ と HLG の表示の色空間は線形の 1.0 を 100 cd/m² に置くので、`mm-color-ocio` はこの 2 つを `bt2020` と同じく `mm-color-lut` で作業色空間に直してから渡す。

### `mm-color-api`

- `TransformProgram::Lut3d` に `shaper: LutShaper` を足す（`#[serde(default)]`、既定は `None`）。`LutShaper::Pq` は線形の光を PQ の符号化値にしてから格子を引く（0..10000 cd/m² が 0..1）。線形の光を入力にする変換（`lin_rec709`、OCIO の `Linear ...`、`ACEScg`、`ACES2065-1`）をベイクするときに使う。
- `REC2020_TO_REC709` と `REC709_TO_REC2020` は `mm_types::transfer` に移し、`mm-color-api` から再公開する（デコーダーも使うため）。

`Lut3d` の形が変わるので、パターンで中を取り出す呼び出し側は `..` か `shaper` を足す。シリアライズした `Lut3d`（`shaper` の無いもの）はそのまま読める。

### `mm-types`（足すだけ）

- `transfer`: `pq_to_linear`、`linear_to_pq`、`hlg_to_linear`、`linear_to_hlg`、`HDR_REFERENCE_WHITE_NITS`、`PQ_PEAK_NITS`、`HLG_PEAK_NITS`、`mul3`、原色の行列。WGSL の `mm_pq_to_linear` などと `mm_rec2020_to_rec709`。
- `tonemap`: `bt2390_to_sdr`、`preview_peak`、`TONEMAP_WGSL`。
- `ColorSpace::{bt2100_pq, bt2100_hlg}`。

### 実装（インターフェースは変えない）

- デコード: CPU の経路は 10 bit 以上、HDR、Rec.2020 の原色の素材を 8 bit を経由せずに線形にする（swscale は色差の補間だけ）。GPU の経路（`d3d12va`、`cuvid`、YUV のプレーンを上げる `ffmpeg`）は共通の YUV の WGSL が伝達関数と原色を引く。Rec.2020 の原色の素材は作業色空間（Rec.709）に行列で直す（RFC 0005 の決めたことを、デコーダーで実際に掛ける）。
- 書き出し: `hevc_pq` と `hevc_hlg`（libx265、HEVC Main10、MP4）。線形を Rec.2020 の PQ か HLG にして 10 bit の YUV を自前で作る。HDR10 のマスタリングの表示は P3-D65 の 1000 cd/m²、MaxCLL / MaxFALL は 1000 / 400 の固定。SDR のコーデックは今と同じ経路。
- プレビュー: `mm-ui-wgpu` の Program と Source が BT.2390 の EETF で SDR の表示に収める。素材の上限は `Ui::set_hdr_peak`（既定 1.0 で恒等）。

## 影響

- `mm-color-api` の `Lut3d` を取り出すパターンは `..` が要る（このリポジトリの呼び出し側は直した）。
- bt2020 の SDR の素材は、デコーダーが原色を Rec.709 に直すようになり、見た目が変わる（今までは直していなかった）。
- 10 bit の SDR の素材は CPU の経路で 8 bit を経由しなくなり、画素の値がわずかに変わる。
- フレームのキャッシュのキーは変えない。デコーダーの出す画素が変わる素材（HDR、10 bit、bt2020）は、キャッシュを消すまで古い画素が残りうる。

## 残したこと

- MaxCLL / MaxFALL を描いた画素から測る（ヘッダーの前に全フレームを描く 2 パスが要る）。
- NVENC（`hevc_nvenc`）の HDR の書き出し。
- HDR の表示（HDR のサーフェスと Windows の HDR の表示）。
- アプリの再生の経路から `Ui::set_hdr_peak(tonemap::preview_peak(素材の色空間))` を呼ぶこと（app-shell の担当）。
- `mm frame` の PNG（Claude の目）のトーンマップ。
