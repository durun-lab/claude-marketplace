//! FFmpeg のライブラリによるデコーダー（`mm-decode-api` の実装）。
//!
//! - フレームの番号は、開くときに読む映像パケットの PTS の索引の位置で決める。B フレーム、
//!   シーク後、VFR でも同じ素材は同じ番号になる。
//! - 素材の時間軸の 0 は最初のフレームの PTS。コンテナの開始時刻が 0 でなくてもフレーム 0 が t = 0。
//! - 固定フレームレート（CFR）なら `fps` を返し、フレームの `pts` は `index / fps`。
//!   そうでなければ VFR として `pts_index` に各フレームの時刻を返す。
//! - 壊れた素材は `Err(Corrupt)`。
//!
//! feature `ffmpeg` を入れたときだけ実装をビルドする（FFmpeg の開発ファイルが要るため）。
#![cfg(feature = "ffmpeg")]

mod audio;
mod convert;
mod sound_only;
#[cfg(feature = "gpu")]
mod upload;
mod video;
mod wide;

use std::path::Path;
use std::sync::OnceLock;

use mm_decode_api::{Decoder, DecoderFactory, GpuContextRef};
use mm_types::{MmError, Result};

pub use sound_only::SoundOnlyDecoder;
pub use video::FfmpegDecoder;

/// 実装の名前。`mm-runtime` の設定で選ぶ。
pub const ID: &str = "ffmpeg";
/// 広い信号の素材を読むときに名乗る名前。画素の作り方のバージョンが付く。
pub const WIDE_ID: &str = mm_decode_ffmpeg_core::wide_backend_id!("ffmpeg");
/// 広い信号の素材の YUV のプレーンを GPU に上げて線形にするときに名乗る名前（feature `gpu`）。CPU の経路と
/// 色差の補間と丸めが違い画素が一致しないので、フレームのキャッシュで互いの画像を引かないよう名前を分ける。
pub const UPLOAD_WIDE_ID: &str = mm_decode_ffmpeg_core::wide_backend_id!("ffmpeg-upload");
/// FFmpeg のデコーダー（映像と音だけの素材）が名乗る名前（`Decoder::backend_id`）の全部。
pub const BACKEND_IDS: [&str; 3] = [ID, WIDE_ID, UPLOAD_WIDE_ID];

/// FFmpeg を開くデコーダーのインターフェース。
#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegDecoderFactory;

impl DecoderFactory for FfmpegDecoderFactory {
    fn id(&self) -> &'static str {
        ID
    }

    fn backend_ids(&self) -> Vec<&'static str> {
        BACKEND_IDS.to_vec()
    }

    /// デコードは CPU。feature `gpu` で `mm-gpu` のコンテキストを渡されたときは、YUV のプレーンを上げて
    /// GPU のフレームで返す。それ以外は CPU の RGBA。
    fn open(&self, path: &Path, gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        init()?;
        #[cfg(feature = "gpu")]
        if let Some(ctx) = gpu.and_then(|g| g.as_any().downcast_ref::<mm_gpu::GpuContext>()) {
            return or_sound_only(path, FfmpegDecoder::open_with_gpu(path, ctx));
        }
        let _ = gpu;
        or_sound_only(path, FfmpegDecoder::open(path))
    }
}

/// 映像のデコーダーが開けない（`Unsupported`）素材は、映像の無い素材として開き直す。
/// 映像のある素材を 2 回開かないよう、映像のデコーダーが断ったときだけ調べる。
fn or_sound_only(path: &Path, opened: Result<FfmpegDecoder>) -> Result<Box<dyn Decoder>> {
    match opened {
        Ok(dec) => Ok(Box::new(dec)),
        Err(MmError::Unsupported(reason)) => match SoundOnlyDecoder::open_if_sound_only(path)? {
            Some(dec) => Ok(Box::new(dec)),
            None => Err(MmError::Unsupported(reason)),
        },
        Err(e) => Err(e),
    }
}

/// GPU のコンテキストを渡されても CPU の RGBA で返すインターフェース。経路の比較の計測と、GPU に上げない確かめに使う。
#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegCpuDecoderFactory;

impl DecoderFactory for FfmpegCpuDecoderFactory {
    fn id(&self) -> &'static str {
        "ffmpeg_cpu"
    }

    fn backend_ids(&self) -> Vec<&'static str> {
        BACKEND_IDS.to_vec()
    }

    fn open(&self, path: &Path, _gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        init()?;
        or_sound_only(path, FfmpegDecoder::open(path))
    }
}

/// FFmpeg の初期化を 1 回だけ行う。
pub fn init() -> Result<()> {
    static INIT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| {
        ffmpeg_next::init().map_err(|e| e.to_string())?;
        ffmpeg_next::util::log::set_level(ffmpeg_next::util::log::Level::Error);
        Ok(())
    })
    .clone()
    .map_err(|e| MmError::Internal(format!("FFmpeg の初期化: {e}")))
}
