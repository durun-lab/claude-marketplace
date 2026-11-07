//! コーデックの指定。どのエンコーダーを、どのコンテナに、どの画素形式と設定で書くか。
//! 実装のクレート（`mm-encode-ffmpeg`、`mm-encode-nvenc`）が名前から作り、共通実装はこれだけを見て書く。

/// 出力のコンテナ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// 配信向け。先頭に moov を置く（`+faststart`）。
    Mp4,
    /// ProRes と DNxHR の置き場。
    Mov,
    /// 配信の区間。MPEG-TS（HLS の区間）。
    MpegTs,
}

impl Container {
    /// FFmpeg の muxer の名前。拡張子に依らずこれで書く。
    pub fn muxer(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::MpegTs => "mpegts",
        }
    }
}

/// エンコーダーに渡す画素形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoInput {
    /// 8 ビットの 4:2:0。GPU で作った NV12 をそのまま送れる。
    Nv12,
    /// 8 ビットの 4:2:2。DNxHR の LB、SQ、HQ。
    Yuv422p8,
    /// 10 ビットの 4:2:2。ProRes 422 と DNxHR HQX。
    Yuv422p10,
    /// 10 ビットの 4:2:0。HEVC Main10（HDR）。
    Yuv420p10,
    /// 10 ビットの 4:4:4。ProRes 4444 と DNxHR 444。
    Yuv444p10,
}

/// 書き出す信号の伝達特性と原色。線形の作業色空間（Rec.709）から、この信号の符号化値にしてエンコーダーに渡す。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputSignal {
    /// Rec.709 の原色、Rec.1886 の表示の式（SDR）。1 を超える値は白に切る。
    #[default]
    Sdr,
    /// Rec.2020 の原色、PQ（SMPTE ST 2084、HDR10）。線形の 1.0 が 203 cd/m²。
    Pq,
    /// Rec.2020 の原色、HLG（ARIB STD-B67）。表示の上限 1000 cd/m² の OOTF を外して符号化する。
    Hlg,
}

impl OutputSignal {
    /// HDR か。
    pub fn is_hdr(self) -> bool {
        self != OutputSignal::Sdr
    }
}

/// HDR10 のマスタリングの表示とコンテンツの明るさ。x265 の `master-display` と `max-cll` にそのまま書く。
/// 表示は P3-D65 の 1000 cd/m²（多くの HDR10 の納品の既定）、MaxCLL / MaxFALL は表示の上限と、その 4 割。
/// 描いた画素から測って書く形は、ヘッダーの前に全フレームを描く 2 パスが要るので、まだ固定の値で持つ。
pub const HDR10_MASTER_DISPLAY: &str =
    "G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1)";
/// HDR10 の MaxCLL と MaxFALL（cd/m²）。
pub const HDR10_MAX_CLL: &str = "1000,400";

impl VideoInput {
    /// NV12 の 8 ビットの中間を経由せず、線形の f32 から 16 ビットの符号化値を作って変換する形式か（NV12 以外の全部）。
    pub fn is_high_bit_depth(self) -> bool {
        self != VideoInput::Nv12
    }
}

/// 音のコーデック。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// AAC 192 kbps。配信向け。
    Aac,
    /// 24 ビットの PCM。ProRes と DNxHR の納品向け。
    PcmS24,
}

/// ビットレートの決め方。
#[derive(Debug, Clone, PartialEq)]
pub enum Rate {
    /// 品質を一定にする。値はエンコーダーの `crf` または `cq` の名前と値。
    Quality(&'static str, &'static str),
    /// 1 画素 1 フレームあたりのビット数。大きさと fps から目標のビットレートを決める。2 パスはこちら。
    BitsPerPixel(f64),
    /// 目標のビットレートと上限は実行時に `StreamOutput` で渡る（配信の区間）。
    Stream,
    /// コーデックの設定（ProRes と DNxHR の profile）で決まる。
    Profile,
}

/// 1 つのコーデックの書き方。
#[derive(Debug, Clone, PartialEq)]
pub struct CodecSpec {
    /// FFmpeg のエンコーダーの名前（`libx264`、`prores_ks`、`dnxhd`、`h264_nvenc`、`hevc_nvenc`）。
    pub encoder: &'static str,
    pub container: Container,
    pub input: VideoInput,
    pub audio: AudioCodec,
    pub rate: Rate,
    /// エンコーダーに渡すオプション（preset、profile など）。
    pub options: Vec<(&'static str, String)>,
    /// 映像を通す回数。2 のときは 1 回目で統計を取り、2 回目で書く。
    pub passes: u32,
    /// ストリームの codec_tag（HEVC を QuickTime で開くための `hvc1` など）。
    pub codec_tag: Option<[u8; 4]>,
    /// 書き出す信号。既定は SDR（Rec.709）。
    pub signal: OutputSignal,
}

impl CodecSpec {
    /// 1 パスの指定。`options` と `codec_tag` は後から足す。
    pub fn new(
        encoder: &'static str,
        container: Container,
        input: VideoInput,
        audio: AudioCodec,
        rate: Rate,
    ) -> Self {
        Self {
            encoder,
            container,
            input,
            audio,
            rate,
            options: Vec::new(),
            passes: 1,
            codec_tag: None,
            signal: OutputSignal::Sdr,
        }
    }

    /// 書き出す信号を決める。
    pub fn with_signal(mut self, signal: OutputSignal) -> Self {
        self.signal = signal;
        self
    }

    /// エンコーダーのオプションを足す。
    pub fn option(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.options.push((key, value.into()));
        self
    }

    /// 映像を通す回数を決める。
    pub fn with_passes(mut self, passes: u32) -> Self {
        self.passes = passes;
        self
    }

    /// ストリームの codec_tag を決める。
    pub fn with_codec_tag(mut self, tag: [u8; 4]) -> Self {
        self.codec_tag = Some(tag);
        self
    }
}
