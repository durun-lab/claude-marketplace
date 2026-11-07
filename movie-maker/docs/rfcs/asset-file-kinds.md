# RFC: 素材のファイルを読む種類

状態: 提案

## 変える理由

段階 8 の `lottie`（`mm-nodes-lottie`）は、プロジェクトの素材の Lottie の JSON をパラメータ（`AssetRef`）で指し、ファイルを自分で読む。今のレンダラーは、ふつうのレイヤーのパラメータに `AssetRef` があると素材をデコーダーで開く。JSON は開けないのでレイヤーが誤りになる。また、ふつうのレイヤーに渡す入力（`NoInputs`）は `InputFrames::asset_path` に答えないので、種類から素材のパスを引けない（効果の `color.lut` は効果の経路なので引ける）。

## 足したもの（形は変えない）

| 型 | 足したもの | 既定 |
|---|---|---|
| `NodeKind`（`mm-node-api`） | `fn decodes_asset(&self) -> bool`。パラメータの `AssetRef` の素材を、レンダラーがデコーダーで開いて `FrameCx::footage` に渡すか | `true`（今の振る舞い） |

`NODE_API_VERSION` は上げない（既定の実装つきのメソッドで、既存の種類は書き換えない）。

## 振る舞いが変わる所（`mm-render`）

- `kind_image`（ふつうのレイヤー）は、`decodes_asset` が偽の種類では素材を開かない。`FrameCx::footage` は `None`。
- `kind_image` が渡す入力は、効果と同じくプロジェクトのデータから `asset_path` に答える（入力の映像は 0 本のまま）。`footage` や `image` にも渡るが、どちらも `asset_path` を引かないので画素は変わらない。
- フレームのキャッシュのキーは今のまま。`AssetRef` の素材のコンテンツハッシュがキーに入るので、素材を差し替えればキャッシュは外れる。

## 先行事例

| 先行事例 | 採ったもの |
|---|---|
| OpenFX の性質の宣言（`kOfxImageEffectPropSupportsTiles` など。ホストは宣言を見て渡し方を変える） | 種類が「素材をデコーダーで開くか」を宣言し、レンダラーがそれに従う。既定は今の振る舞いにして、宣言しない種類は変わらない |

## 使う所

- `mm-nodes-lottie` の `LottieKind`: `decodes_asset` を偽にし、`asset_path` で JSON のパスを引く。
- 後の `svg`（`mm-nodes-basic` の予定）も同じ形で SVG のファイルを読める。
