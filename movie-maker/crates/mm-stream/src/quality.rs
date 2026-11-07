//! HLS の画質。短い辺の画素数とビットレートの目標。

use std::str::FromStr;

use mm_types::{MmError, Rational, Result};

/// 音の AAC のビットレート（bit/s）。
pub(crate) const AUDIO_BITS_PER_SECOND: u32 = 64_000;
/// fps の上限。これを超える素材は半分にして重ねる。
const MAX_FPS: u32 = 30;
/// MPEG-TS の PES とトランスポートのヘッダーが、符号化したストリームに足すバイトの割合（百分率）。
const TS_OVERHEAD_PERCENT: u32 = 20;

/// 映像の上限（maxrate）の、目標に対する割合（百分率）。
const MAXRATE_PERCENT: u32 = 115;
/// 4 秒の区間の最大が maxrate を超える割合（百分率）。VBV のバッファーが区間ごとに満タンから始まるため。
const VBV_PEAK_PERCENT: u32 = 145;

/// `value` の `percent` パーセント。
fn scale(value: u32, percent: u32) -> u32 {
    (u64::from(value) * u64::from(percent) / 100) as u32
}

/// HLS の画質。画面の切り替えで選ぶ（既定は 360p）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HlsQuality {
    /// 短い辺 360 画素、映像と音で約 0.5 Mbps。
    #[default]
    P360,
    /// 短い辺 540 画素、映像と音で約 1.2 Mbps。
    P540,
}

impl HlsQuality {
    /// 選べる画質。親のプレイリストが並べる順。
    pub const ALL: [HlsQuality; 2] = [HlsQuality::P360, HlsQuality::P540];

    /// URL とキャッシュのパスに使う名前。
    pub fn as_str(self) -> &'static str {
        match self {
            HlsQuality::P360 => "360p",
            HlsQuality::P540 => "540p",
        }
    }

    /// 短い辺の画素数。
    pub fn short_side(self) -> u32 {
        match self {
            HlsQuality::P360 => 360,
            HlsQuality::P540 => 540,
        }
    }

    /// 映像と音を合わせたビットレートの目標（bit/s）。
    pub fn total_bits_per_second(self) -> u32 {
        match self {
            HlsQuality::P360 => 500_000,
            HlsQuality::P540 => 1_200_000,
        }
    }

    /// 映像だけのビットレートの目標（bit/s）。MPEG-TS のヘッダーが足す分を引いて、
    /// 届くバイト数が全体の目標に収まるようにする。
    pub(crate) fn video_bits_per_second(self) -> u32 {
        self.total_bits_per_second() * 100 / (100 + TS_OVERHEAD_PERCENT) - AUDIO_BITS_PER_SECOND
    }

    /// 映像のビットレートの上限（VBV の maxrate、bit/s）。区間が短いので、目標からの上振れを小さく抑える。
    pub(crate) fn video_max_bits_per_second(self) -> u32 {
        scale(self.video_bits_per_second(), MAXRATE_PERCENT)
    }

    /// プレイリストの `BANDWIDTH`（bit/s）。区間のピークのビットレートで、VBV が区間ごとに満タンから始まるぶんの
    /// 上振れ（上限の約 1.45 倍）と音と MPEG-TS のヘッダーを見込む。
    pub fn peak_bits_per_second(self) -> u32 {
        let video_peak = scale(self.video_max_bits_per_second(), VBV_PEAK_PERCENT);
        scale(
            video_peak + AUDIO_BITS_PER_SECOND,
            100 + TS_OVERHEAD_PERCENT,
        )
    }

    /// 元の大きさの縦横比を保ち、短い辺をこの画質にした大きさ（偶数）。
    pub(crate) fn size_for(self, width: u32, height: u32) -> (u32, u32) {
        let short = self.short_side();
        let (w, h) = (u64::from(width.max(1)), u64::from(height.max(1)));
        let even = |v: u64| v.div_ceil(2) * 2;
        if w >= h {
            (even((u64::from(short) * w + h / 2) / h) as u32, short)
        } else {
            (short, even((u64::from(short) * h + w / 2) / w) as u32)
        }
    }
}

impl FromStr for HlsQuality {
    type Err = MmError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "360" | "360p" => Ok(HlsQuality::P360),
            "540" | "540p" => Ok(HlsQuality::P540),
            other => Err(MmError::InvalidArgument(format!(
                "画質は 360p か 540p: {other}"
            ))),
        }
    }
}

/// 配信の fps。30 を超える間は半分にする（60 → 30、59.94 → 29.97）。
pub(crate) fn stream_fps(source: Rational) -> Rational {
    let (mut num, mut den) = (source.num(), source.den());
    while f64::from(num) / f64::from(den) > f64::from(MAX_FPS) + 0.5 {
        if num.is_multiple_of(2) {
            num /= 2;
        } else {
            den = den.saturating_mul(2);
        }
    }
    Rational::new(num, den).unwrap_or(Rational::integer(MAX_FPS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 短い辺を画質にそろえて偶数にする() {
        assert_eq!(HlsQuality::P360.size_for(1920, 1080), (640, 360));
        assert_eq!(HlsQuality::P360.size_for(1080, 1920), (360, 640));
        assert_eq!(HlsQuality::P540.size_for(1920, 1080), (960, 540));
        assert_eq!(HlsQuality::P360.size_for(1000, 1000), (360, 360));
        assert_eq!(HlsQuality::P360.size_for(2560, 1080).0 % 2, 0);
    }

    #[test]
    fn fps_は_30_を超えたら半分にする() {
        let f = |n, d| stream_fps(Rational::new(n, d).unwrap());
        assert_eq!(f(60, 1), Rational::integer(30));
        assert_eq!(f(60000, 1001), Rational::new(30000, 1001).unwrap());
        assert_eq!(f(24, 1), Rational::integer(24));
        assert_eq!(f(30000, 1001), Rational::new(30000, 1001).unwrap());
        assert_eq!(f(120, 1), Rational::integer(30));
    }

    #[test]
    fn bandwidth_は目標より大きく_vbv_の上振れを見込む() {
        let total = HlsQuality::P360.total_bits_per_second();
        let peak = HlsQuality::P360.peak_bits_per_second();
        assert!(peak > total * 3 / 2 && peak < total * 2, "{peak}");
        assert!(HlsQuality::P540.peak_bits_per_second() > HlsQuality::P360.peak_bits_per_second());
    }

    #[test]
    fn 名前から画質を読む() {
        assert_eq!("360p".parse::<HlsQuality>().unwrap(), HlsQuality::P360);
        assert!("720p".parse::<HlsQuality>().is_err());
    }
}
