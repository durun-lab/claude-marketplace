# RFC 0180 hdr-sdr-match: HDR の素材があるとき、SDR の書き出しにプレビューと同じトーンマップを掛ける

HDR の素材があるとき、プレビューは BT.2390 の EETF で SDR の表示に収め、SDR の書き出しは 1 で切っていた。プレビューで見た映像と書き出した映像が違う。SDR の書き出しにもプレビューと同じトーンマップを掛ける。そのために素材の上限の明るさをエンコーダーに渡す。足すだけの変更（フィールドと関数の追加）で、HDR の素材の無いプロジェクトの書き出しは変わらない。決めた理由と先行事例は `docs/design/03-data-model.md` の「色」。

## 追加

`mm-decode-api`:

| 追加 | 内容 |
|---|---|
| `EncodeSettings::source_peak: f32` | 素材の上限の明るさ（線形、1.0 が HDR の基準の白）。SDR の信号の書き出しは `mm_types::tonemap::bt2390_to_sdr` でこれを白に収める。1 以下は恒等。HDR の信号の書き出しは使わない。serde の既定は 1.0 |

`mm-render-api`:

| 追加 | 内容 |
|---|---|
| `tone_map_peak(project) -> f32` | プロジェクトの素材の色空間から素材の上限の明るさを決める（`tonemap::hdr_source_peak`）。プレビュー（`Ui::set_hdr_peak`）、書き出し（`EncodeSettings::source_peak`、GPU の NV12 の変換）、配信の区間のキーが同じこの値を使う |

素材の上限の明るさは `begin` の前に呼ぶメソッドにせず、`EncodeSettings` に入れる（RFC 0156 の `stream` と同じ理由）。包むエンコーダー（`mm-runtime` の `AutoEncoder`、`mm-export` の `Metered` と並列のチャンク）は `begin` の設定をそのまま渡すので、転送を足さなくてよい。既存の構造体の literal には `source_peak: 1.0` を足した。

`mm-types`（API クレートではない）の `tonemap::preview_peak` は、書き出しも使うので `hdr_source_peak` に名前を変える。

## 実装

- GPU の NV12 の経路（`mm-render` の `export.rs`）: NV12 は SDR なので、`TONEMAP_WGSL` を連結し、一様バッファの値でトーンマップしてから Rec.1886 にする。値はレンダラーが `tone_map_peak(req.project)` で決める。
- CPU の経路（`mm-encode-ffmpeg-core` の `fill.rs`）: SDR の 8 bit と 16 bit の符号化の前に `bt2390_to_sdr(rgb, source_peak)` を掛ける。PQ と HLG の書き出しは掛けない。
- アプリ（`apps/movie-maker`）: プロジェクトが変わるたびに `Ui::set_hdr_peak(tone_map_peak(project))` を呼ぶ。
- 配信の区間のキー（`mm-stream`）: 素材の上限の明るさは区間に置いていない素材でも変わるので、キーに入れ、区間の作り方のバージョンを上げる。フレームのキャッシュの画像はトーンマップの前の線形なので、キーは変えない。

## 残したこと

- HDR の素材があるプロジェクトでは、SDR の文字や画像の白が 8 bit の tv レンジで 235 から 214 に下がる。全体に掛けるトーンマップでは、白を 235 に置いたままハイライトを残せない（super-white はファイルの再生で切られる）。235 に戻すには HDR の素材の入力の側のトーンマップが要り、別の RFC で決める。理由と先行事例は `03-data-model.md` の「色」。
- 切る（トーンマップしない）書き出しを選ぶプロジェクトの設定は足さない。DaVinci Resolve の出力のトーンマップの `None` に当たる。要る用途が出たら `colorConfig` に足す。
- `mm frame` と `GET /frame` の PNG（Claude の目。`mm-serve` の `encode_png`）のトーンマップ。`mm-serve` は claude-entry の領域。
