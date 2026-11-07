# RFC 0156 encode-abr-ts: 目標のビットレートの H.264 と MPEG-TS の出力

`mm-stream` は HLS の区間を、中間の MP4 に書いてから ffmpeg の CLI で符号化し直している。符号化を 1 回にするため、`mm-decode-api` の `EncodeSettings` に配信の区間の書き方を足す。足すだけの変更（フィールドと型の追加）で、既存の実装の動きは変わらない。

## 追加

`mm-decode-api`:

| 追加 | 内容 |
|---|---|
| `StreamOutput` | `video_bit_rate`、`video_max_rate`（VBV の上限、`None` は無し）、`audio_bit_rate`、`keyframe_interval_frames`、`start_offset`（`RationalTime`） |
| `EncodeSettings::stream: Option<StreamOutput>` | コーデック `h264_ts` に要り、ほかでは `None`。serde は既定値つき（`None` は書かない） |

設定は `begin` の前に呼ぶメソッド（`set_extras` の形）にせず、`EncodeSettings` に入れる。目標のビットレートとキーフレームの間隔はコーデックと同じく `begin` で決まる設定で、FFmpeg の `AVCodecContext` や NVENC の初期化の引数も 1 つの組で渡す。包むエンコーダー（`mm-runtime` の `AutoEncoder`、`mm-export` の `Metered`）は `begin` の設定をそのまま渡すので、転送を足さなくてよい。既存の構造体の literal には `stream: None` を足した（開いている PR に新しい literal を足すものは無い）。

## 実装

- コーデック `h264_ts`（`mm-encode-ffmpeg`）: libx264、AAC、MPEG-TS。`stream` が要る。ほかのコーデックに `stream` を渡すと `begin` が `InvalidArgument`。
- `mm-encode-ffmpeg-core`: `Container::MpegTs`、`Rate::Stream`、音の AAC のビットレートを指定できる。CRF、2 パス、ProRes、DNxHR の経路は変えない。
- キーフレームは `keyframe_interval_frames` ごとの固定間隔（`g` と `keyint_min` を同じ値、`sc_threshold=0`）。区間の先頭が必ず IDR になる。
- B フレームは使わない。DTS が PTS より前に出ないので、TS の時刻が `start_offset` からずれずに始まる。
- AAC の前置き（1024 サンプル、約 21 ms）のパケットは MPEG-TS では捨てる。MP4 のような edit list が無く、残すと前の区間の終わりと重なり、区間 0 では負の時刻のために muxer が全部のストリームを後ろへずらす。音も映像も `start_offset` から始まる。
- VBV のバッファーは上限の 2 秒分（`mm-stream` の CLI の `bufsize` = 目標の 2 倍と同じ程度）。

## 残したこと

- 区間の終わりの AAC のフレームは 1024 サンプル単位に丸まり、次の区間の頭と 1 フレーム未満（48 kHz の 4 秒の区間で 512 サンプル、約 11 ms）重なる。区間ごとに別に符号化するので、境で MDCT の重なりがつながらず、ごく小さな不連続も残る。なくすには区間をまたいで 1 本で符号化して切る必要があり、`mm-stream` の作り方（見られた区間だけを作る）と合わない。
- VBV は区間ごとに満ちた状態から始まるので、4 秒の区間のビットレートの最大は上限の約 1.45 倍になりうる。プレイリストの `BANDWIDTH` はこれを見込む。
- B フレームを使わない分、同じビットレートの画質は少し落ちる。
- `mm-export` の `ExportOptions` から `EncodeSettings::stream` を渡す経路は、`mm-stream` を切り替える PR で足す。
