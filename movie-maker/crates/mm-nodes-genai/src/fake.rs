//! 偽の提供元。通信せず、頼みから決まる単色の画像・動画と正弦波の音を返す。試験と、提供元を決める前の通しに使う。

use std::sync::atomic::{AtomicUsize, Ordering};

use mm_types::{Rational, Result};

use crate::encode::{png_solid, wav_sine, y4m_solid};
use crate::provider::{Estimate, Generated, Provider};
use crate::request::{GenRequest, Media};

/// 偽の提供元の名前。`gen.ai` の `provider` の既定値。
pub const FAKE_PROVIDER: &str = "fake";
/// 偽の動画のフレームレート。小さく保ち、試験のファイルを軽くする。
pub const FAKE_FPS: u32 = 10;
const FAKE_REVISION: &str = "fake-1";
const AMPLITUDE: f32 = 0.25;
const BASE_HZ: f32 = 220.0;
const HZ_STEPS: u8 = 220;
const MS_PER_SEC: f64 = 1000.0;
const OPAQUE: u8 = u8::MAX;

/// 偽の料金表。実際のサービスに近い桁にして、見積もりの表示と承認の上限を試せるようにする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FakePrices {
    /// 画像 1 枚の費用（米ドル）。
    pub per_image_usd: f64,
    /// 動画 1 秒の費用（米ドル）。
    pub per_video_second_usd: f64,
    /// 音 1 秒の費用（米ドル）。
    pub per_audio_second_usd: f64,
}

impl Default for FakePrices {
    fn default() -> Self {
        Self {
            per_image_usd: 0.04,
            per_video_second_usd: 0.10,
            per_audio_second_usd: 0.02,
        }
    }
}

/// 偽の提供元。`generate` を呼んだ回数を数え、使い回しで呼ばないことを試験で確かめられる。
#[derive(Debug, Default)]
pub struct FakeProvider {
    prices: FakePrices,
    calls: AtomicUsize,
}

impl FakeProvider {
    /// 既定の料金表で。
    pub fn new() -> Self {
        Self::default()
    }

    /// 料金表を変える。
    pub fn with_prices(mut self, prices: FakePrices) -> Self {
        self.prices = prices;
        self
    }

    /// `generate` を呼んだ回数。
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// 頼みから決まる 3 バイト。プロンプトとシードが同じなら同じ。
    pub fn tint(req: &GenRequest) -> [u8; 3] {
        let mut h = blake3::Hasher::new();
        h.update(req.prompt.as_bytes());
        h.update(&req.seed.to_le_bytes());
        let b = h.finalize();
        let b = b.as_bytes();
        [b[0], b[1], b[2]]
    }

    /// 偽の動画のフレーム数。
    pub fn video_frames(req: &GenRequest) -> u32 {
        (req.duration_ms as f64 * f64::from(FAKE_FPS) / MS_PER_SEC).ceil() as u32
    }

    /// 偽の音の高さ（Hz）。
    pub fn tone_hz(req: &GenRequest) -> f32 {
        BASE_HZ + f32::from(Self::tint(req)[0] % HZ_STEPS)
    }
}

impl Provider for FakeProvider {
    fn id(&self) -> &'static str {
        FAKE_PROVIDER
    }

    fn revision(&self) -> &str {
        FAKE_REVISION
    }

    fn supports(&self, _media: Media) -> bool {
        true
    }

    fn estimate(&self, req: &GenRequest) -> Result<Estimate> {
        let secs = req.duration_ms as f64 / MS_PER_SEC;
        let cost_usd = match req.media {
            Media::Image => self.prices.per_image_usd,
            Media::Video => self.prices.per_video_second_usd * secs,
            Media::Audio => self.prices.per_audio_second_usd * secs,
        };
        Ok(Estimate {
            cost_usd,
            seconds: 0.0,
        })
    }

    fn generate(&self, req: &GenRequest) -> Result<Generated> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let [a, b, c] = Self::tint(req);
        let (bytes, extension, fps) = match req.media {
            Media::Image => (
                png_solid(req.width, req.height, [a, b, c, OPAQUE])?,
                "png",
                None,
            ),
            Media::Video => (
                y4m_solid(
                    req.width,
                    req.height,
                    FAKE_FPS,
                    Self::video_frames(req),
                    [a, b, c],
                ),
                "y4m",
                Some(Rational::integer(FAKE_FPS)),
            ),
            Media::Audio => (
                wav_sine(req.duration_ms, Self::tone_hz(req), AMPLITUDE),
                "wav",
                None,
            ),
        };
        Ok(Generated {
            bytes,
            extension,
            duration: req.duration(),
            fps,
        })
    }
}
