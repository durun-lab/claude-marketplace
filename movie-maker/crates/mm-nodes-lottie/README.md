# mm-nodes-lottie

`lottie` のノードの種類。プロジェクトの素材の Lottie（JSON）を、レイヤーの時刻で決めたフレームで CPU に描き、レイヤーにする（[04-nodes.md](../../docs/design/04-nodes.md) の `lottie`、[07-standards.md](../../docs/design/07-standards.md) の Lottie 1.0）。
読み込みは [velato](https://github.com/linebender/velato)、描画は [vello_cpu](https://github.com/linebender/vello/tree/main/sparse_strips/vello_cpu)。返す画像は線形の premultiplied の f32 で、レンダラーが `Rgba16Float` のテクスチャにする。

## パラメータ

| 名前 | 型 | 既定 | 意味 |
|---|---|---|---|
| `animation` | 素材 | 空 | Lottie の JSON の素材。空なら何も描かない |
| `speed` | 数（0〜16） | 1 | 再生の速さ。0 なら `start_frame` で止まる |
| `loop` | 真偽 | 真 | 最後のフレームの次に先頭へ戻るか。偽なら最後のフレームで止まる |
| `start_frame` | 数（0〜） | 0 | レイヤーの先頭で見せるフレーム（アニメーションの先頭 `ip` からの数） |
| `fit` | 選択 | `contain` | `contain` は縦横比を保って画布に収め、中央に置く。`none` はアニメーションの画素のまま左上に置く |

- 時刻はレイヤーの素材の時間軸（`FrameCx::source_time`。レイヤーの先頭が 0 で、トリムと速さとタイムリマップはレンダラーが済ませてある）。フレームは `ip + start_frame + 秒 × fr × speed` で、端数のフレームもそのまま描く。
- 素材は `addAsset` で足す（デコーダーで調べる取り込みは通さない）。種類は `decodes_asset` を偽にし、レンダラーは素材を開かずに `InputFrames::asset_path` でパスを渡す（[RFC 0134](../../docs/rfcs/0134-asset-file-kinds.md)）。
- 大きさはレイヤーの画布（`cx.size`）。`vector` の種類なので、レイヤーを拡大すると拡大した大きさで描き直す。
- 色: vello_cpu は Lottie の色を符号化値のまま 8 bit で塗る。その値を素材と同じ Rec.1886 の式で線形にする（半透明は alpha で割ってから線形にし、掛け直す）。

## 実装の選定

### 先行事例の比較

| 実装 | 言語と束縛 | Windows（MSVC）のビルド | Lottie の対応 | CPU で描けるか | ライセンス |
|---|---|---|---|---|---|
| velato 0.12 + vello_cpu 0.3（linebender） | pure Rust | `cargo build` だけ。C/C++ の道具も libclang も要らない | シェイプ、トランスフォーム、マスク、マット、プリコンポジションなど主要部。上流の README が挙げる未対応: 位置のキーフレームの `ti`/`to` の補間、タイムリマップ（`tm`）、テキスト、埋め込みの画像、破線などの高度なシェイプ、効果、分割した位置と回転 | 可。velato の `RenderSink` を実装すれば描画先を選べる（vello の連携は feature で切れる） | MIT か Apache-2.0 |
| rlottie（Samsung、`rlottie` クレート） | C++、bindgen | rlottie の本体（vcpkg か自前のビルド）と pkg-config、libclang が要る | Telegram のステッカーで使われ広い。テキストと一部の効果は未対応 | 可（ARGB32 の premultiplied のバッファ） | MIT（同梱の第三者のコードは個別） |
| ThorVG（`thorvg` クレート、dotlottie-rs の中身） | C++、bindgen | C++ のビルド（vendored）が要る。Rust の束縛は個人の保守 | 広い（テキスト、式も） | 可（ソフトウェアのラスタライザ） | MIT |
| lottie-rs（zimond） | pure Rust | | 描画は bevy の経路で、2024 年から更新が無い | 不可（GPU の bevy） | MIT か Apache-2.0 |

### 決めたこと

- velato で読み、`RenderSink` を vello_cpu の `RenderContext` に流す（`src/raster.rs`）。決め手は 3 つ。
  - ビルドに C/C++ の道具が要らず、CI と手元の Windows で追加の準備が無い（rlottie と ThorVG は libclang と C++ のビルドが要る）。
  - vello の連携を切れるので、wgpu を 2 つの版で抱えない（ワークスペースの wgpu と vello の wgpu は版が違う）。
  - CPU だけで描くので画素が GPU のバックエンドによらず、期待画像が 1 組で済む。決定性も単純に保てる。
- 対応の範囲の不足（上の表）は上流の改善を待つ。足りない機能の多い素材が増えたら、同じ `RenderSink` の位置で ThorVG に差し替える（種類のパラメータとキャッシュは変えない）。
- vello_cpu は単一スレッドで描く。`multithreading` の feature は、この大きさでは描くたびのスレッドの受け渡しが重く、手元の 1080p で 1 フレーム 18 ms から 52 ms に遅くなった。

### 新規に設計したもの

- フレームの写像（`src/timing.rs`）。ループは `ip..op` の剰余、ループしないときは `op - 1` で止める（`op` は最後のフレームの次）。
- キャッシュ（`src/cache.rs`）。読んだアニメーションはパスと更新時刻で 4 つ、描いたフレームはパス、更新時刻、決めたフレーム、大きさ、置き方の変形で 8 枚まで覚える。時刻ではなく決めたフレームで持つので、ループの 2 周目と速さ 0 は描き直さない。レンダラーのフレームのキャッシュ（パラメータ、時刻、大きさ、素材のコンテンツハッシュ）はこの外にある。
- 埋め込みの画像（`assets` の画像）は描かずに飛ばす。ほかのレイヤーは描く。
- 符号化値から線形への変換は、alpha と値の組 256×256 の表を 1 回だけ作って引く。半透明の画素ごとに `powf` を呼ぶと 1080p で 25 ms かかった。

## 速さ

`cargo run --release -p mm-nodes-lottie --example frame_time [JSON]`。試験のフィクスチャ（3 つのシェイプのレイヤー、グラデーション、60% の不透明度）の 1920x1080 で、1 フレーム 11〜14 ms（単一スレッド、毎回違う時刻）。大半は 1920x1080 の f32 の画像（33 MB）の確保と変換で、vello_cpu の描画は数 ms。

## 試験

`tests/it/`。

- `contract.rs`: ノードの契約（既定値と、アニメーションを描くとき）。
- `kind.rs`: 決定性と覚え、ループ、止まる、開始のフレーム、速さ、置き方、壊れた JSON。
- `golden.rs`: フィクスチャを t = 0 と 1.25 秒で描き、`tests/it/golden/vello_cpu/` と比べる。更新は `MM_GOLDEN_UPDATE=1`。
- `render.rs`: 操作（MCP のツールと同じ JSON の `addAsset`、`addLayer`）で置いたレイヤーを wgpu のレンダラーで描く。壊れた JSON はそのレイヤーだけ誤りになる。組み立てたエンジンの種類の一覧に入る。

フィクスチャ（`tests/it/fixtures/`）は手で書いた。
