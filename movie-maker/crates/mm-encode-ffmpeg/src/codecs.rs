//! コーデックの名前から書き方（`CodecSpec`）を決める表。

use mm_encode_ffmpeg_core::{
    AudioCodec, CodecSpec, Container, OutputSignal, Rate, VideoInput, HDR10_MASTER_DISPLAY,
    HDR10_MAX_CLL,
};

/// 扱う名前（エイリアスを含む）。
pub const SUPPORTED_NAMES: [&str; 16] = [
    "h264",
    "h264_ts",
    "libx264",
    "h264_2pass",
    "hevc_hlg",
    "hevc_pq",
    "prores",
    "prores_ks",
    "prores_422hq",
    "prores_4444",
    "dnxhr_lb",
    "dnxhr_sq",
    "dnxhr_hq",
    "dnxhr_hqx",
    "dnxhr_444",
    "dnxhd",
];

/// x264 の品質（小さいほど高画質）。編集の途中の確認と配信の両方に足りる値。
const X264_CRF: &str = "18";
const X264_PRESET: &str = "medium";
/// 配信の区間（`h264_ts`）の profile と level。親のプレイリストの `CODECS`（`avc1.640028`）と同じ値にそろえる。
/// level 4.0 は 540p の縦横比の広い映像（1 フレーム 8192 マクロブロックまで）まで収める。
const H264_TS_PROFILE: &str = "high";
const H264_TS_LEVEL: &str = "40";
/// 2 パスの目標。1 画素 1 フレームあたりのビット数（1080p60 でおよそ 15 Mbps）。
const TWO_PASS_BITS_PER_PIXEL: f64 = 0.12;
/// ProRes の profile（prores_ks）。
const PRORES_HQ: &str = "hq";
const PRORES_4444: &str = "4444";
/// ProRes の vendor。QuickTime が Apple のエンコーダーとして扱う。
const PRORES_VENDOR: &str = "apl0";
/// x265 の品質（x264 の 18 とおよそ同じ見た目）。
const X265_CRF: &str = "20";
/// HEVC を QuickTime と Apple の端末で開くための codec_tag。
const HVC1: [u8; 4] = *b"hvc1";

/// 名前から指定を決める。扱えない名前は `None`。
pub fn spec_of(codec: &str) -> Option<CodecSpec> {
    match codec {
        "h264" | "libx264" => Some(x264()),
        "h264_ts" => Some(
            CodecSpec::new(
                "libx264",
                Container::MpegTs,
                VideoInput::Nv12,
                AudioCodec::Aac,
                Rate::Stream,
            )
            .option("preset", X264_PRESET)
            .option("profile", H264_TS_PROFILE)
            .option("level", H264_TS_LEVEL),
        ),
        "h264_2pass" => Some(
            CodecSpec::new(
                "libx264",
                Container::Mp4,
                VideoInput::Nv12,
                AudioCodec::Aac,
                Rate::BitsPerPixel(TWO_PASS_BITS_PER_PIXEL),
            )
            .option("preset", X264_PRESET)
            .with_passes(2),
        ),
        "hevc_pq" => Some(x265_hdr(OutputSignal::Pq)),
        "hevc_hlg" => Some(x265_hdr(OutputSignal::Hlg)),
        "prores" | "prores_ks" | "prores_422hq" => Some(prores(PRORES_HQ, VideoInput::Yuv422p10)),
        "prores_4444" => Some(prores(PRORES_4444, VideoInput::Yuv444p10)),
        "dnxhd" | "dnxhr_hq" => Some(dnxhr("dnxhr_hq", VideoInput::Yuv422p8)),
        "dnxhr_lb" => Some(dnxhr("dnxhr_lb", VideoInput::Yuv422p8)),
        "dnxhr_sq" => Some(dnxhr("dnxhr_sq", VideoInput::Yuv422p8)),
        "dnxhr_hqx" => Some(dnxhr("dnxhr_hqx", VideoInput::Yuv422p10)),
        "dnxhr_444" => Some(dnxhr("dnxhr_444", VideoInput::Yuv444p10)),
        _ => None,
    }
}

fn x264() -> CodecSpec {
    CodecSpec::new(
        "libx264",
        Container::Mp4,
        VideoInput::Nv12,
        AudioCodec::Aac,
        Rate::Quality("crf", X264_CRF),
    )
    .option("preset", X264_PRESET)
}

/// HEVC Main10 の HDR（libx265）。色の宣言は VUI に、HDR10 のマスタリングの表示と MaxCLL / MaxFALL は SEI に書く。
fn x265_hdr(signal: OutputSignal) -> CodecSpec {
    let params = match signal {
        OutputSignal::Pq => format!(
            "hdr10=1:repeat-headers=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:\
             master-display={HDR10_MASTER_DISPLAY}:max-cll={HDR10_MAX_CLL}"
        ),
        _ => "repeat-headers=1:colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc".into(),
    };
    CodecSpec::new(
        "libx265",
        Container::Mp4,
        VideoInput::Yuv420p10,
        AudioCodec::Aac,
        Rate::Quality("crf", X265_CRF),
    )
    .option("preset", X264_PRESET)
    .option("x265-params", params)
    .with_codec_tag(HVC1)
    .with_signal(signal)
}

fn prores(profile: &str, input: VideoInput) -> CodecSpec {
    CodecSpec::new(
        "prores_ks",
        Container::Mov,
        input,
        AudioCodec::PcmS24,
        Rate::Profile,
    )
    .option("profile", profile)
    .option("vendor", PRORES_VENDOR)
}

fn dnxhr(profile: &str, input: VideoInput) -> CodecSpec {
    CodecSpec::new(
        "dnxhd",
        Container::Mov,
        input,
        AudioCodec::PcmS24,
        Rate::Profile,
    )
    .option("profile", profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 扱う名前はすべて指定になる() {
        for name in SUPPORTED_NAMES {
            assert!(spec_of(name).is_some(), "{name}");
        }
        assert!(spec_of("h264_nvenc").is_none(), "NVENC は別のクレート");
        assert!(spec_of("vp9").is_none());
    }

    /// プロジェクトの移行と書き出しの警告は `mm_doc::HDR_SIGNAL_CODECS` で HDR の信号の書き出しを見分ける。
    #[test]
    fn hdr_の信号で書き出す名前はプロジェクトのデータの表と同じ() {
        let hdr: Vec<&str> = SUPPORTED_NAMES
            .into_iter()
            .filter(|n| spec_of(n).is_some_and(|s| s.signal.is_hdr()))
            .collect();
        assert_eq!(hdr, mm_doc::HDR_SIGNAL_CODECS);
    }

    #[test]
    fn コンテナと画素形式はコーデックで決まる() {
        let x = spec_of("h264").unwrap();
        assert_eq!(
            (x.container, x.input, x.passes),
            (Container::Mp4, VideoInput::Nv12, 1)
        );
        let ts = spec_of("h264_ts").unwrap();
        assert_eq!(
            (ts.container, ts.rate.clone()),
            (Container::MpegTs, Rate::Stream)
        );
        let two = spec_of("h264_2pass").unwrap();
        assert_eq!(two.passes, 2);
        assert!(
            matches!(two.rate, Rate::BitsPerPixel(_)),
            "2 パスは目標のビットレート"
        );
        let p = spec_of("prores_4444").unwrap();
        assert_eq!(
            (p.container, p.input),
            (Container::Mov, VideoInput::Yuv444p10)
        );
        let d = spec_of("dnxhr_hqx").unwrap();
        assert_eq!((d.encoder, d.input), ("dnxhd", VideoInput::Yuv422p10));
        assert_eq!(spec_of("dnxhr_hq").unwrap().input, VideoInput::Yuv422p8);
        let pq = spec_of("hevc_pq").unwrap();
        assert_eq!(
            (pq.encoder, pq.input, pq.signal),
            ("libx265", VideoInput::Yuv420p10, OutputSignal::Pq)
        );
        assert_eq!(spec_of("hevc_hlg").unwrap().signal, OutputSignal::Hlg);
        assert_eq!(x.signal, OutputSignal::Sdr, "SDR の書き出しは変えない");
    }
}
