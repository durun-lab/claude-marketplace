//! テストの部品。`gen.ai` のレイヤーを置いたプロジェクト、モックの提供元、生成した PNG を読む素材の供給元。

use std::path::Path;
use std::sync::Arc;

use mm_audio_api::AudioBlock;
use mm_decode_api::{DecodedFrame, FrameSource, MediaInfo};
use mm_doc::{Composition, Layer, Node};
use mm_nodes_genai::{FakeProvider, GenAi, Providers, GENAI_KIND, KIND_VERSION};
use mm_ops::{AddComposition, AddLayer, Editor, OpContext, Transaction};
use mm_types::{
    ColorSpace, Id, LayerId, MmError, ParamValue, Rational, RationalTime, Result, Rgba, RgbaImage,
    Size, TimeRange,
};

pub const COMP: &str = "main";
pub const LAYER: &str = "g";
pub const WIDTH: f64 = 32.0;
pub const HEIGHT: f64 = 18.0;
const FPS: u32 = 30;
const SECONDS: i64 = 5;

/// モックの提供元 1 つの `GenAi` と、その提供元（呼んだ回数を見る）。
pub fn genai(media_dir: &Path) -> (GenAi, Arc<FakeProvider>) {
    let fake = Arc::new(FakeProvider::new());
    let providers = Providers::new().with(fake.clone()).expect("1 つ");
    (GenAi::new(media_dir, providers), fake)
}

/// `gen.ai` のノード。小さな画像を既定にし、`extra` で上書きする。
pub fn genai_node(extra: &[(&str, ParamValue)]) -> Node {
    let mut node = Node::new(GENAI_KIND, KIND_VERSION)
        .with_param("prompt", ParamValue::Text("夕焼けの海".into()))
        .with_param("width", ParamValue::Number(WIDTH))
        .with_param("height", ParamValue::Number(HEIGHT));
    for (k, v) in extra {
        node = node.with_param(*k, v.clone());
    }
    node
}

/// コンポジションを足し、`gen.ai` のレイヤーを置いた編集の状態。
pub fn editor_with_layer(node: Node) -> (Editor, LayerId) {
    let mut ed = Editor::new(mm_doc::Project::new());
    let comp = Composition::new(
        Id::new(COMP),
        WIDTH as u32,
        HEIGHT as u32,
        Rational::integer(FPS),
        RationalTime::seconds(SECONDS),
    );
    let layer = Layer::new(
        LAYER,
        node,
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(SECONDS),
    );
    let ops = vec![
        AddComposition { composition: comp }.into(),
        AddLayer {
            composition: Id::new(COMP),
            layer,
            index: None,
        }
        .into(),
    ];
    ed.apply(Transaction::new("置く", ops), &OpContext::human())
        .expect("置ける");
    (ed, Id::new(LAYER))
}

/// 生成した PNG を読んで 1 枚の画像として返す素材の供給元。FFmpeg の静止画のデコーダーの代わり。
pub struct PngSource {
    info: MediaInfo,
    image: RgbaImage,
}

impl PngSource {
    /// `path` の PNG を読む。画素は 0..1 の straight のまま（不透明の単色を比べるだけなので変換しない）。
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .map_err(|e| MmError::Corrupt(e.to_string()))?;
        let mut buf = vec![0; reader.output_buffer_size().expect("大きさ")];
        let frame = reader
            .next_frame(&mut buf)
            .map_err(|e| MmError::Corrupt(e.to_string()))?;
        let data = buf[..frame.buffer_size()]
            .iter()
            .map(|b| f32::from(*b) / f32::from(u8::MAX))
            .collect();
        let size = Size::new(frame.width, frame.height);
        Ok(Self {
            info: MediaInfo {
                duration: RationalTime::ZERO,
                fps: None,
                size,
                color_space: ColorSpace::srgb(),
                timebase_offset: RationalTime::ZERO,
                audio: None,
                pts_index: Vec::new(),
            },
            image: RgbaImage::from_raw(size, data)?,
        })
    }
}

impl FrameSource for PngSource {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn frame_at(&mut self, t: RationalTime) -> Result<DecodedFrame> {
        Ok(DecodedFrame::cpu(0, t, self.image.clone()))
    }

    fn audio_in(&mut self, range: TimeRange) -> Result<AudioBlock> {
        Err(MmError::NotFound(format!("音が無い: {range:?}")))
    }
}

/// モックの提供元の色（不透明）。
pub fn fake_color(tint: [u8; 3]) -> Rgba {
    let c = |b: u8| f32::from(b) / f32::from(u8::MAX);
    Rgba::new(c(tint[0]), c(tint[1]), c(tint[2]), 1.0)
}
