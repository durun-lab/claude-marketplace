//! `mm-encode-ffmpeg` の結合テスト。リンクを 1 回にするため、1 つの実行ファイルにまとめる（docs/ci.md）。

#[cfg(feature = "ffmpeg")]
mod codecs;
#[cfg(feature = "ffmpeg")]
#[path = "../../../mm-decode-ffmpeg/tests/it/common/mod.rs"]
mod common;
#[cfg(feature = "ffmpeg")]
mod contract;
#[cfg(feature = "ffmpeg")]
mod hdr;
#[cfg(feature = "ffmpeg")]
mod roundtrip;
#[cfg(feature = "ffmpeg")]
mod sdr_tone_map;
#[cfg(feature = "ffmpeg")]
mod stream_ts;
