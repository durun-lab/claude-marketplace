//! モバイル版への映像の配信（docs/design/12-mobile.md の「映像の届け方」）。
//!
//! 3 本の道を、HTTP を知らない関数として出す。`mm-serve` のハンドラーが [`Streamer`] を呼び、
//! 返った [`Payload`]（状態、ヘッダー、本体）をそのまま返す。URL の形と認可はハンドラーの担当。
//!
//! - 静止画のコマ（[`Streamer::still`]）: 指定の時刻のプレビューを WebP にする。
//! - 区間ごとに作る HLS（[`Streamer::playlist`]、[`Streamer::segment`]）: 要求された区間だけを描き、
//!   360p 約 0.5 Mbps の H.264 と AAC の MPEG-TS にする。要求の後ろの数区間は先に作る。
//! - 素材の音だけ（[`Streamer::audio`]）: AAC 64 kbps の m4a。`Range` に答える。
//!
//! 描画は専用のスレッドが持つ `Engine` で行い、アプリの再生と `GET /frame` を止めない。
//! 区間とコマのキャッシュはプロジェクトの版で分け、版が変わると捨てる。

mod audio;
mod cache;
mod frames;
mod hls;
mod payload;
mod quality;
mod segment;
mod streamer;
mod tools;

pub use audio::AudioRequest;
pub use frames::{StillRequest, STILL_MAX_WIDTH, STILL_MIN_WIDTH};
pub use hls::{HlsRequest, SegmentName, SEGMENT_SECONDS};
pub use payload::{ByteRange, Payload};
pub use quality::HlsQuality;
pub use streamer::{StreamConfig, Streamer};
