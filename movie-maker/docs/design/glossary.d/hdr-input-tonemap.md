## HDR の素材のトーンマップの位置

| 用語 | 定義 | 識別子 |
|---|---|---|
| タイムラインの作業の明るさ | プロジェクトの合成を SDR の範囲（1.0 が白）で行うか、HDR の範囲（1 を超える値を持ち回る）で行うかの設定。既定は SDR。HDR の素材のトーンマップを掛ける位置を決める。「作業の明るさ」と略してよい | `colorConfig.workingRange`、`WorkingRange::{Sdr, Hdr}`、`setColorConfig` |
| 入力の側のトーンマップ | 作業の明るさが SDR のとき、HDR の素材のレイヤーの画像に効果と合成の前で掛ける SDR のトーンマップ。SDR の素材と文字は通らない。Premiere Pro の「メディアを自動でトーンマップ」に当たる | `input_tone_map_peak`、`input_tone_mapped`、`mm-render` の `input_tone.rs` |
| 合成の後のトーンマップ | 作業の明るさが HDR のとき、SDR の出力（プレビュー、SDR の書き出し、PNG、モバイルの静止画）が合成したフレームに掛ける SDR のトーンマップ | `tone_map_peak` |
| HDR の信号のコーデック | HDR の信号（PQ、HLG）で書き出すレンダーターゲットのコーデック。既存のプロジェクトの作業の明るさの移行と、書き出しの警告が見分けに使う | `HDR_SIGNAL_CODECS`、`hdr_signal_from_sdr_range`、`20261108_working_range` |
| 設定の指紋（フレームのキャッシュの） | 色の変換の設定、文字の実装、書体の組を表す文字列。フレームのキャッシュのキーに入る。素材の短いハッシュの「指紋」とは別 | `WgpuRenderer::config_fingerprint`、`key_color` |
