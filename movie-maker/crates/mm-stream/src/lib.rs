//! モバイル版への映像の配信（docs/design/12-mobile.md の「映像の届け方」）。
//!
//! 4 つの配信を、HTTP を知らない関数として出す。`mm-serve` のハンドラーが [`Streamer`] を呼び、
//! 返った [`Payload`]（状態、ヘッダー、本体）をそのまま返す。URL の形と認可はハンドラーの担当。
//!
//! - 静止画のフレーム（[`Streamer::still`]）: 指定の時刻のプレビューを WebP にする。
//! - 区間ごとに作る HLS（[`Streamer::playlist`]、[`Streamer::segment`]）: コンポジションを 4 秒の配信の区間に割り、
//!   要求された区間だけを描いて、360p 約 0.5 Mbps の H.264 と AAC の MPEG-TS に 1 回で符号化する。要求の後ろの数区間は先に作る。
//!   区間のキー（[`SegmentKey`]）は区間に掛かるものの指紋で、編集で変わらない区間は作り直さない。
//! - 素材の音だけ（[`Streamer::audio`]）: AAC 64 kbps の m4a。`Range` に答える。
//! - 提案の区間の短い動画（[`Streamer::clip`]）: 提案を適用したプロジェクトの、提案の範囲の前後 2 秒を足した所を
//!   360p の H.264 と AAC の MP4（moov が先頭）にする。`Range` に答える。
//!
//! 描画は専用のスレッドが持つ 1 つの `Engine` で行い、アプリの再生を止めない（[`Streamer::with_engine`]）。

mod audio;
mod cache;
mod clip;
mod frames;
mod hls;
mod key;
#[cfg(test)]
mod key_tests;
mod listing;
mod payload;
mod quality;
mod segment;
mod streamer;
mod tools;
mod worker;

pub use audio::AudioRequest;
pub use clip::{ClipRequest, CLIP_MAX_SECONDS, CLIP_PAD_SECONDS};
pub use frames::{StillRequest, STILL_MAX_WIDTH, STILL_MIN_WIDTH};
pub use hls::{HlsRequest, SEGMENT_SECONDS};
pub use key::SegmentKey;
pub use payload::{ByteRange, Payload};
pub use quality::HlsQuality;
pub use streamer::{StreamConfig, Streamer};
