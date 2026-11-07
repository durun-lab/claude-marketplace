# mm-nodes-hlsl

`shader.hlsl` のノードの種類。利用者が書いた HLSL のピクセルシェーダーで 1 枚を描く（[04-nodes.md](../../docs/design/04-nodes.md) の `shader.hlsl`）。
naga は HLSL を直接読めないので、DXC で SPIR-V にしてから naga で WGSL にし、`shader.wgsl` と同じ形（全画面の三角形、線形の premultiplied、`Rgba16Float`）で描く。

## パラメータと入力

| 名前 | 型 | 既定 | 意味 |
|---|---|---|---|
| `code` | 文字 | UV をそのまま色にする | HLSL の源 |
| `asset` | 素材 | 空 | HLSL の源のファイル。空でなければ `code` より優先する |
| `p0`..`p7` | 数 | 0 | シェーダーに渡す数。`u.p0`（`p0`..`p3`）、`u.p1`（`p4`..`p7`） |

入力は `source` と `other` の 2 つで、どちらも省ける。2 つを繋ぐときは同じ大きさにする。出力の大きさは `source` の大きさ（無ければ画布）。

## HLSL の書き方

`float4 mm_main(float2 uv)` を定義し、線形の premultiplied の色を返す。`uv` は左上が (0, 0)。種類が前置きと入口を足すので、利用者は宣言を書かない。

```hlsl
float4 mm_main(float2 uv) {
    float4 c = mm_input(uv);
    return float4(c.a - c.rgb, c.a); // 色の反転
}
```

使える名前（`shader.wgsl` と同じ名前）:

| 名前 | 中身 |
|---|---|
| `u.size` | 出力の画素数（`float2`） |
| `u.time` | 秒 |
| `u.scale` | 画面上の倍率 |
| `u.p0`、`u.p1` | `p0`..`p3`、`p4`..`p7`（`float4`） |
| `mm_input(uv)`、`mm_input1(uv)` | 入力 0 と 1 を双線形で引く（範囲の外は端の色）。繋がっていなければ透明 |
| `mm_t0`、`mm_t1`、`mm_samp` | 入力のテクスチャとサンプラー（自分で `Sample` を呼ぶとき） |

前置きが宣言するレジスタと、wgpu の束ね方（`@group(0)`）の対応:

| HLSL | 型 | binding | DXC の指定 |
|---|---|---|---|
| `register(b0)` | `ConstantBuffer<MmU> u` | 0 | そのまま |
| `register(t0)`、`register(t1)` | `Texture2D<float4> mm_t0`、`mm_t1` | 1、2 | `-fvk-t-shift 1 0` |
| `register(s0)` | `SamplerState mm_samp` | 入力の数 + 1 | `-fvk-s-shift <入力の数 + 1> 0` |

uniform の形（64 バイト、`shader.wgsl` の `struct U` と同じ並び）:

```hlsl
struct MmU {
    float2 size;  // offset 0
    float time;   // offset 8
    float scale;  // offset 12
    float4 p0;    // offset 16
    float4 p1;    // offset 32
};
```

約束:

- `ps_6_0`、HLSL 2021 で変換する。入口は種類が足す `mm_fs`（`TEXCOORD0` の `uv` を受ける）。
- 繋がっていない入力のテクスチャは宣言されない。`mm_t1` を直接使うコードは、入力を 2 つ繋いだときだけ通る。
- naga が SPIR-V を読めない機能（`printf` など）は、変換の誤りになる。

## 誤り

変換の失敗は、そのレイヤーだけの誤り（`InvalidArgument`）になり、レンダラーがレイヤーを透明にして記録する。
本文は `<源の名前>:<行>:<列>: <本文>` の形で、行と列は利用者の源の中で数える（前置きの後に `#line 1` を置く）。源の名前は `code`、素材ならファイル名。

| 段 | 例 | 行の出し方 |
|---|---|---|
| DXC | `DXC が HLSL を SPIR-V にできない: code:3:16: use of undeclared identifier 'nothere'` | DXC の誤りの行をそのまま |
| naga | `naga が SPIR-V を WGSL にできない: code:3:5: SPIR-V を読めない: unsupported extension set NonSemantic.DebugPrintf` | DXC に `-fspv-debug=line` で `OpLine` を付けさせ、naga がつまずいた命令の直前の `OpLine` を拾う |

DXC が無い環境では、描くたびに `Unsupported` を返す（種類は登録したままで、そのレイヤーだけが誤り）。

## DXC の入手と呼び方

| 選択肢 | SPIR-V | ビルド | 配布 | 決定性 |
|---|---|---|---|---|
| Windows SDK の `dxc.exe` | 出せない（SDK 10.0.26100 の 1.8.2502 で `SPIR-V CodeGen not available`） | 要らない | SDK に入っている | 使えない |
| DirectXShaderCompiler の配布の `dxc.exe` を別プロセスで呼ぶ（採用） | 出せる | 要らない | `dxc.exe` と `dxcompiler.dll`（x64 で 23 MB）を置く。winget の `Microsoft.DirectX.ShaderCompiler` で入る | 版を固定すれば同じ出力。版は変換のキャッシュのキーに入れる |
| 同じ配布の `dxcompiler.dll` をプロセスの中で呼ぶ（hassle-rs など） | 出せる | COM の結びつけの依存が増える | `dxcompiler.dll` を置く | 同じ |
| Vulkan SDK の `dxc.exe` | 出せる | 要らない | SDK は数百 MB で、利用者に入れてもらうには重い | 同じ |

別プロセスを選んだ理由:

- DXC が落ちても描画のプロセスを巻き込まない。CEF や OFX と同じく、外のコードは別プロセスに置く方針に揃う。
- 起動の時間（下の「速さ」）は、源のハッシュのキャッシュで初回だけになる。プロセスの中で呼んでも DXC の変換そのものの時間は残る。
- 決定性: 一時のフォルダーで相対の名前（`shader.hlsl`）を渡し、SPIR-V に一時のパスが入らないようにする。同じ源と版からは同じ SPIR-V になる（`dxc.rs` の試験）。

探す順: 環境変数 `MM_DXC`（実行ファイルのパス）、winget のパッケージ（`%LOCALAPPDATA%\Microsoft\WinGet\Packages\Microsoft.DirectX.ShaderCompiler_*\bin\x64\dxc.exe`）、`PATH`。
各候補で小さな源を 1 つ SPIR-V にしてみて、通った最初のものを使う（`PATH` で先に当たる Windows SDK の `dxc.exe` を飛ばす）。探すのはプロセスで 1 回。

```sh
winget install --id Microsoft.DirectX.ShaderCompiler -e
```

## 先行事例と新規の設計

| 先行事例 | 採ったもの |
|---|---|
| wgpu と naga の HLSL の扱い（naga は HLSL を書けるが読めない。HLSL から入るときは DXC で SPIR-V を作る） | DXC → SPIR-V → naga → WGSL の 2 段 |
| DXC の Vulkan の束ね方（`-fvk-*-shift` で `b`、`t`、`s` の番号を 1 つの set の binding にずらす） | `shader.wgsl` の binding（0 が uniform、1.. が入力、その次がサンプラー）にずらして揃える |
| Shadertoy、gl-transitions、`shader.wgsl`（利用者は 1 つの関数だけを書き、前置きが uniform と入力を与える） | `mm_main(uv)` だけを書かせ、名前（`u.time`、`mm_input`）を `shader.wgsl` と同じにする |

新規に設計したもの:

- naga の誤りの行。naga の SPIR-V の読み込みは命令の種類しか返さないので、`OpLine` を自前でたどって利用者の行に戻す。naga は型の宣言の間の `OpLine` を読めないので、naga に渡す前に `OpLine` を落とす。
- キャッシュ。変換は DXC の版、入力の数、全文の blake3 で引き、失敗も含めてプロセスで 64 件まで覚える（並行の書き出しはワーカーごとに組み立てるので、種類ごとではなくプロセスで持つ）。パイプラインは種類ごとに WGSL の全文で 32 件まで。
- 源の文字コード。DXC は源を SPIR-V に埋めるとき、BOM の無い UTF-8 の ASCII の外の文字（日本語のコメント）で失敗するので、BOM を付けて渡す。

## 速さ

RTX 5090、DXC 1.9.2609（`tests/it/convert.rs` の `同じ源は_2_回目から変換し直さない`、`time.hlsl`）。

| 場合 | 時間 |
|---|---|
| 初回の変換（DXC の起動、SPIR-V、naga、WGSL） | 50〜65 ms |
| キャッシュにヒットする | 0.02 ms |

## 試す

```sh
MM_REQUIRE_GPU=1 MM_REQUIRE_DXC=1 cargo nextest run -p mm-nodes-hlsl
```

- `tests/it/render.rs`: フィクスチャ（色の反転、2 つの入力を混ぜる、時刻で動く縞）を描き、同じことをする `shader.wgsl`、`transition.wgsl` と画素が一致すること。
- `tests/it/convert.rs`: DXC と naga の誤りの行、キャッシュ、入力の数ごとの束ね方。
- `tests/it/golden.rs`: 期待画像 `fixtures/golden/rtx/shader-hlsl.png`。更新は `MM_GOLDEN_UPDATE=1` を付けて回し、PNG を目で確かめる。

DXC が無い環境では変換の試験をスキップし、`MM_REQUIRE_DXC` があれば失敗にする。
