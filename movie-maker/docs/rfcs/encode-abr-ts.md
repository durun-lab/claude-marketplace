# RFC encode-abr-ts: 目標のビットレートの H.264 と MPEG-TS の出力

`mm-stream` は HLS の区間を、中間の MP4 に書いてから ffmpeg の CLI で符号化し直している。符号化を 1 回にするため、`mm-decode-api` の `Encoder` に配信の区間の書き方を渡す口を足す。足すだけの変更（既定の実装つきのメソッドと型の追加）で、既存の実装と呼び出しは変わらない。

## 追加

`mm-decode-api`:

| 追加 | 内容 |
|---|---|
| `StreamOutput` | `video_bit_rate`、`video_max_rate`（VBV の上限、`None` は無し）、`audio_bit_rate`、`keyframe_interval_frames`、`start_offset`（`RationalTime`） |
| `Encoder::set_stream_output(StreamOutput)` | `begin` の前に呼ぶ。既定は `Err(Unsupported)` |

`EncodeSettings` は変えない（構造体の literal を増やさないため、設定は `set_extras` と同じ形のメソッドで渡す）。

## 実装

- コーデック `h264_ts`（`mm-encode-ffmpeg`）: libx264、AAC、MPEG-TS。`set_stream_output` が要る。ほかのコーデックに渡すと `begin` が `InvalidArgument`。
- `mm-encode-ffmpeg-core`: `Container::MpegTs`、`Rate::Stream`、音の AAC のビットレートを指定できる。CRF、2 パス、ProRes、DNxHR の経路は変えない。
- キーフレームは `keyframe_interval_frames` ごとの固定間隔（シーンチェンジなし）。B フレームは使わず、DTS と PTS が一致するので、出力の時刻は `start_offset` から正確に始まる。

## 影響

- `Encoder` の実装のうち `mm-runtime` の `AutoEncoder` と `mm-export` の `Metered` は `set_stream_output` を転送しない（既定の `Unsupported` を返す）。`mm-stream` の切り替えのときに、使う経路に合わせて転送を足す。
