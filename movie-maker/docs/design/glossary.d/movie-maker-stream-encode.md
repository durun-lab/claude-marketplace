## 配信の区間の符号化

| 用語 | 定義 | 識別子 |
|---|---|---|
| 親のプレイリスト | 画質ごとのプレイリストを並べ、画質ごとの `BANDWIDTH`、`AVERAGE-BANDWIDTH`、大きさ、fps を載せる HLS のプレイリスト（RFC 8216 の Master Playlist） | `Streamer::master_playlist`、`hls::master_playlist` |
| 区間のピークのビットレート | 親のプレイリストの `BANDWIDTH`。VBV が許す 1 つの配信の区間の最大のビットレートに、音と MPEG-TS のヘッダーを足した値 | `HlsQuality::peak_bits_per_second` |
