//! 映像のトラック: 線形の RGBA の f32 → エンコーダーの画素形式（SDR は Rec.709、HDR は Rec.2020 の PQ か HLG。
//! tv レンジ）→ エンコーダー。レンダラーが GPU で作った NV12 は、NV12 を受けるコーデックならそのまま送る。

use std::path::PathBuf;

use ff::color::{Primaries, Range, Space, TransferCharacteristic};
use ff::format::context::Output;
use ff::format::Pixel;
use ff::software::scaling::{Context as Scaler, Flags};
use ff::{codec, encoder, ffi, frame, Rational};
use ffmpeg_next as ff;
use mm_decode_api::{EncodeSettings, Nv12Frame, StreamOutput};
use mm_types::{MmError, Result, RgbaImage, Size};

use crate::fill::{Encoding, Prepared};
use crate::spec::{CodecSpec, OutputSignal, Rate, VideoInput};

/// 何回目の映像か。2 パスの 1 回目は出力に書かず統計だけを取る。
#[derive(Debug, Clone)]
pub enum Pass {
    Single,
    /// 統計ファイルのパス。
    First(PathBuf),
    Second(PathBuf),
}

/// 映像のエンコーダーと、出力の中のストリーム。
pub struct VideoTrack {
    encoder: codec::encoder::video::Encoder,
    scaler: Scaler,
    /// 1 回目のパスは出力を持たない。
    stream_index: Option<usize>,
    encoder_tb: Rational,
    stream_tb: Rational,
    size: Size,
    next_pts: i64,
    input: VideoInput,
    /// 線形 → 符号化値。スケーラーに渡す前にエンコードし直す。
    encoding: Encoding,
}

impl VideoTrack {
    /// 出力にストリームを足し（`output` があれば）、エンコーダーを開く。
    pub fn open(
        output: Option<&mut Output>,
        settings: &EncodeSettings,
        spec: &CodecSpec,
        pass: &Pass,
        global_header: bool,
    ) -> Result<Self> {
        let size = settings.size;
        if size.width == 0
            || size.height == 0
            || !size.width.is_multiple_of(2)
            || !size.height.is_multiple_of(2)
        {
            return Err(MmError::InvalidArgument(format!(
                "偶数の大きさが要る: {}x{}",
                size.width, size.height
            )));
        }
        let fps = settings.fps;
        let (num, den) = (
            i32::try_from(fps.num()).map_err(|_| too_large("fps"))?,
            i32::try_from(fps.den()).map_err(|_| too_large("fps"))?,
        );
        let codec = encoder::find_by_name(spec.encoder)
            .ok_or_else(|| MmError::Unsupported(format!("{} が無い FFmpeg", spec.encoder)))?;
        let encoder_tb = Rational::new(den, num);
        let opened = open_encoder(codec, settings, spec, pass, encoder_tb, global_header)?;
        let mut stream_index = None;
        let mut stream_tb = encoder_tb;
        if let Some(output) = output {
            let mut stream = output
                .add_stream(codec)
                .map_err(|e| MmError::Internal(format!("映像ストリーム: {e}")))?;
            stream_index = Some(stream.index());
            stream.set_parameters(&opened);
            stream.set_time_base(encoder_tb);
            stream_tb = stream.time_base();
        }
        let encoding = Encoding::for_spec(spec);
        Ok(Self {
            encoder: opened,
            scaler: make_scaler(size, spec, encoding.pixel())?,
            stream_index,
            encoder_tb,
            stream_tb,
            size,
            next_pts: 0,
            input: spec.input,
            encoding,
        })
    }

    /// ヘッダーを書いた後の、muxer が決めたストリームの時間の単位を記録する。
    pub fn after_header(mut self, output: &Output) -> Self {
        if let Some(stream) = self.stream_index.and_then(|i| output.stream(i)) {
            self.stream_tb = stream.time_base();
        }
        self
    }

    /// NV12 をそのまま送れるか。
    pub fn accepts_nv12(&self) -> bool {
        self.input == VideoInput::Nv12
    }

    /// 1 フレームをエンコードして書く。
    pub fn push(&mut self, output: Option<&mut Output>, image: &RgbaImage) -> Result<()> {
        if image.size() != self.size {
            return Err(MmError::InvalidArgument(
                "フレームの大きさが設定と違う".into(),
            ));
        }
        let (w, h) = (self.size.width, self.size.height);
        let yuv = match self.encoding.prepare(image) {
            Prepared::Yuv(yuv) => yuv,
            Prepared::Rgba(rgba) => {
                // エンコーダーが参照を持ち続けるので、フレームごとに新しい YUV の領域を使う
                let mut yuv = frame::Video::new(self.pixel(), w, h);
                self.scaler
                    .run(&rgba, &mut yuv)
                    .map_err(|e| MmError::Internal(format!("YUV への変換: {e}")))?;
                yuv
            }
        };
        self.send(output, yuv)
    }

    /// GPU で作った NV12 の 1 フレームを、変換せずにエンコードして書く。
    pub fn push_nv12(&mut self, output: Option<&mut Output>, nv12: &Nv12Frame) -> Result<()> {
        if nv12.size != self.size {
            return Err(MmError::InvalidArgument(
                "フレームの大きさが設定と違う".into(),
            ));
        }
        let mut yuv = frame::Video::new(Pixel::NV12, self.size.width, self.size.height);
        copy_plane(&mut yuv, 0, &nv12.y, self.size.width as usize);
        copy_plane(&mut yuv, 1, &nv12.uv, self.size.width as usize);
        self.send(output, yuv)
    }

    fn pixel(&self) -> Pixel {
        pixel_of(self.input)
    }

    fn send(&mut self, output: Option<&mut Output>, mut yuv: frame::Video) -> Result<()> {
        yuv.set_pts(Some(self.next_pts));
        self.next_pts += 1;
        self.encoder
            .send_frame(&yuv)
            .map_err(|e| MmError::Internal(format!("映像の送信: {e}")))?;
        self.drain(output)
    }

    /// エンコーダーに残ったフレームを出し切る。
    pub fn finish(&mut self, output: Option<&mut Output>) -> Result<()> {
        self.encoder
            .send_eof()
            .map_err(|e| MmError::Internal(format!("映像の終端: {e}")))?;
        self.drain(output)
    }

    fn drain(&mut self, output: Option<&mut Output>) -> Result<()> {
        crate::writer::drain_packets(
            &mut self.encoder,
            output,
            self.stream_index.unwrap_or(0),
            self.encoder_tb,
            self.stream_tb,
            1,
            false,
        )
    }
}

fn pixel_of(input: VideoInput) -> Pixel {
    match input {
        VideoInput::Nv12 => Pixel::NV12,
        VideoInput::Yuv422p8 => Pixel::YUV422P,
        VideoInput::Yuv422p10 => Pixel::YUV422P10LE,
        VideoInput::Yuv420p10 => Pixel::YUV420P10LE,
        VideoInput::Yuv444p10 => Pixel::YUV444P10LE,
    }
}

/// 行の余白の無いプレーンを、FFmpeg のフレームのプレーン `plane`（行の余白つき）へコピーする。
fn copy_plane(dst: &mut frame::Video, plane: usize, src: &[u8], row: usize) {
    let stride = dst.stride(plane);
    let data = dst.data_mut(plane);
    for (line, out) in src.chunks_exact(row).zip(data.chunks_exact_mut(stride)) {
        out[..row].copy_from_slice(line);
    }
}

/// 目標のビットレート（bit/s）。大きさ x fps x 1 画素 1 フレームあたりのビット数。
fn target_bit_rate(size: Size, fps: f64, bits_per_pixel: f64) -> usize {
    (f64::from(size.width) * f64::from(size.height) * fps * bits_per_pixel) as usize
}

/// 画の大きさ、画素形式、時間の単位、色（信号の原色、伝達特性、行列。tv レンジ）を書く。
fn describe_picture(
    enc: &mut codec::encoder::video::Video,
    size: Size,
    spec: &CodecSpec,
    time_base: Rational,
) {
    enc.set_width(size.width);
    enc.set_height(size.height);
    enc.set_format(pixel_of(spec.input));
    enc.set_time_base(time_base);
    enc.set_frame_rate(Some(Rational::new(
        time_base.denominator(),
        time_base.numerator(),
    )));
    let (space, primaries, trc) = match spec.signal {
        OutputSignal::Sdr => (
            Space::BT709,
            Primaries::BT709,
            TransferCharacteristic::BT709,
        ),
        OutputSignal::Pq => (
            Space::BT2020NCL,
            Primaries::BT2020,
            TransferCharacteristic::SMPTE2084,
        ),
        OutputSignal::Hlg => (
            Space::BT2020NCL,
            Primaries::BT2020,
            TransferCharacteristic::ARIB_STD_B67,
        ),
    };
    enc.set_colorspace(space);
    enc.set_color_range(Range::MPEG);
    enc.set_color_primaries(primaries);
    enc.set_color_transfer_characteristic(trc);
}

/// パス（1 パス、2 パスの 1 回目と 2 回目）とグローバルヘッダーのフラグ、統計ファイルのオプション。
fn pass_options(pass: &Pass, global_header: bool) -> (codec::Flags, ff::Dictionary<'static>) {
    let mut flags = codec::Flags::empty();
    if global_header {
        flags |= codec::Flags::GLOBAL_HEADER;
    }
    let mut options = ff::Dictionary::new();
    let stats = match pass {
        Pass::Single => None,
        Pass::First(stats) => Some((codec::Flags::PASS1, stats)),
        Pass::Second(stats) => Some((codec::Flags::PASS2, stats)),
    };
    if let Some((flag, stats)) = stats {
        flags |= flag;
        options.set("stats", &stats.to_string_lossy());
    }
    (flags, options)
}

/// エンコーダーを信号の色・tv レンジで開く。スレッド数は自動（0）にする。
fn open_encoder(
    codec: ff::Codec,
    settings: &EncodeSettings,
    spec: &CodecSpec,
    pass: &Pass,
    time_base: Rational,
    global_header: bool,
) -> Result<codec::encoder::video::Encoder> {
    let size = settings.size;
    let mut context = codec::context::Context::new_with_codec(codec);
    // 1 スレッドだと 1080p で数倍遅い。フレームの並列は x264 と ProRes のスレッドで効く。
    context.set_threading(ff::threading::Config {
        kind: ff::threading::Type::Frame,
        ..Default::default()
    });
    let mut enc = context
        .encoder()
        .video()
        .map_err(|e| MmError::Internal(format!("映像のエンコーダー: {e}")))?;
    describe_picture(&mut enc, size, spec, time_base);
    let (flags, mut options) = pass_options(pass, global_header);
    enc.set_flags(flags);
    match &spec.rate {
        Rate::Quality(key, value) => options.set(key, value),
        Rate::BitsPerPixel(bpp) => {
            enc.set_bit_rate(target_bit_rate(size, settings.fps.to_f64(), *bpp));
        }
        Rate::Stream => {
            let stream = settings.stream.as_ref().ok_or_else(|| {
                MmError::InvalidArgument(
                    "配信の区間のコーデックには EncodeSettings::stream が要る".into(),
                )
            })?;
            stream_rate_options(&mut options, stream);
        }
        Rate::Profile => {}
    }
    for (key, value) in &spec.options {
        options.set(key, value);
    }
    if let Some(tag) = spec.codec_tag {
        // SAFETY: 開く前のコンテキストの codec_tag に整数を書くだけ。
        unsafe { (*enc.as_mut_ptr()).codec_tag = u32::from_le_bytes(tag) };
    }
    enc.open_with(options)
        .map_err(|e| MmError::Unsupported(format!("{} を開けない: {e}", spec.encoder)))
}

/// VBV のバッファーの長さ（上限のビットレートの何秒分か）。
const VBV_BUFFER_SECS: u32 = 2;

/// 配信の区間の映像のオプション。キーフレームを固定の間隔で入れるのは、区間の先頭を IDR にそろえるため。
fn stream_rate_options(options: &mut ff::Dictionary<'static>, stream: &StreamOutput) {
    options.set("b", &stream.video_bit_rate.to_string());
    if let Some(max) = stream.video_max_rate {
        options.set("maxrate", &max.to_string());
        options.set("bufsize", &max.saturating_mul(VBV_BUFFER_SECS).to_string());
    }
    let gop = stream.keyframe_interval_frames.max(1).to_string();
    options.set("g", &gop);
    options.set("keyint_min", &gop);
    options.set("sc_threshold", "0");
    // B フレームの並べ替えで DTS が PTS より前に出ると、区間の先頭（時刻 0）で TS の時刻が後ろへずれる。
    options.set("bf", "0");
}

fn too_large(what: &str) -> MmError {
    MmError::InvalidArgument(format!("{what} が大きすぎる"))
}

fn make_scaler(size: Size, spec: &CodecSpec, source: Pixel) -> Result<Scaler> {
    let mut scaler = Scaler::get(
        source,
        size.width,
        size.height,
        pixel_of(spec.input),
        size.width,
        size.height,
        Flags::BICUBIC | Flags::ACCURATE_RND | Flags::FULL_CHR_H_INT,
    )
    .map_err(|e| MmError::Internal(format!("swscale: {e}")))?;
    // RGB は フルレンジ、YUV は信号の行列（SDR は Rec.709、HDR は Rec.2020 の非定輝度）の tv レンジ
    let matrix = if spec.signal.is_hdr() {
        ffi::SWS_CS_BT2020
    } else {
        ffi::SWS_CS_ITU709
    };
    unsafe {
        let coeffs = ffi::sws_getCoefficients(matrix);
        ffi::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            coeffs,
            1,
            coeffs,
            0,
            0,
            1 << 16,
            1 << 16,
        );
    }
    Ok(scaler)
}
