## 配信の区間の符号化

| 用語 | 定義 | 識別子 |
|---|---|---|
| 親のプレイリスト | 画質ごとのプレイリストを軽い順に並べ、画質ごとの `BANDWIDTH`、`AVERAGE-BANDWIDTH`、`CODECS`、大きさ、fps を載せる HLS のプレイリスト（RFC 8216 の Master Playlist、改訂版の Multivariant Playlist）。`/m/api/stream/{comp}/index.m3u8` で配る | `Streamer::master_playlist`、`hls::master_playlist`、`MASTER_NAME` |
| 画質ごとのプレイリスト | 1 つの画質の配信の区間を全部並べる VOD のプレイリスト（RFC 8216 の Media Playlist）。親のプレイリストからの相対パス `{quality}.m3u8`。「子」と略さない | `Streamer::playlist`、`Plan::playlist` |
| 区間のピークのビットレート | 親のプレイリストの `BANDWIDTH`。VBV が許す 1 つの配信の区間の最大のビットレートに、音と MPEG-TS のヘッダーを足した値 | `HlsQuality::peak_bits_per_second` |
