//! デコーダーの実装の名前（`Decoder::backend_id`）がフレームのキャッシュのキーに入り、画素の作り方の版が上がった
//! 素材の画像を描き直すこと。版は広い信号の素材を読む実装が名前に付ける（`mm_decode_ffmpeg_core::wide_backend_id`）。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mm_audio_api::AudioBlock;
use mm_decode_api::{DecodedFrame, Decoder, DecoderFactory, GpuContextRef, MediaInfo};
use mm_doc::{Asset, Composition, Layer, Node, Project};
use mm_node_api::Quality;
use mm_nodes_basic::all_kinds;
use mm_render::{GpuContext, GpuRequest, WgpuRenderer};
use mm_render_api::{RenderRequest, Renderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::{SampleDecoderFactory, SampleMedia};
use mm_testkit::fixtures::{sample_services, SAMPLE_ASSET_PATH};
use mm_testkit::node::MapRegistry;
use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime, Result, Size, TimeRange};

const SIZE: Size = Size::new(32, 18);
/// 実装の名前。版の付いた名前は、画素の作り方が変わった広い信号の素材を読むときの名前。
const PLAIN: &str = "tagged";
const VERSIONED: &str = "tagged+wide.1";

/// [`Tagged`] が版の付いた名前を名乗るか。
static VERSION_UP: AtomicBool = AtomicBool::new(false);

fn gpu() -> Option<Arc<GpuContext>> {
    match GpuContext::shared(&GpuRequest::default()) {
        Ok(ctx) => Some(ctx),
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(e) => {
            eprintln!("GPU が無いのでスキップする: {e}");
            None
        }
    }
}

/// モックのデコーダーに、[`VERSION_UP`] で切り替わる実装の名前を付けたもの。
struct Tagged(Box<dyn Decoder>);

impl Decoder for Tagged {
    fn info(&self) -> &MediaInfo {
        self.0.info()
    }

    fn seek(&mut self, t: RationalTime) -> Result<()> {
        self.0.seek(t)
    }

    fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        self.0.next_frame()
    }

    fn decode_audio(&mut self, range: TimeRange) -> Result<AudioBlock> {
        self.0.decode_audio(range)
    }

    fn backend_id(&self) -> Option<&'static str> {
        Some(if VERSION_UP.load(Ordering::SeqCst) {
            VERSIONED
        } else {
            PLAIN
        })
    }
}

struct TaggedFactory;

impl DecoderFactory for TaggedFactory {
    fn id(&self) -> &'static str {
        PLAIN
    }

    fn open(&self, path: &Path, _gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        Ok(Box::new(Tagged(
            SampleDecoderFactory::default().open(path, None)?,
        )))
    }
}

/// 素材を画布の全体に置いたプロジェクト。
fn project() -> Project {
    let media = SampleMedia::default();
    let mut p = Project::new();
    p.assets.push(Asset {
        id: Id::new("clip"),
        path: SAMPLE_ASSET_PATH.into(),
        content_hash: "sample0000000000".into(),
        color_space: ColorSpace::bt709(),
        duration: media.duration,
        fps: Some(media.fps),
        timebase_offset: RationalTime::ZERO,
    });
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(Id::new("clip")));
    let layer = Layer::new(
        "clip-layer",
        footage,
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(1),
    );
    p.compositions.push(
        Composition::new(
            Id::new("main"),
            SIZE.width,
            SIZE.height,
            Rational::integer(30),
            RationalTime::seconds(1),
        )
        .with_layers(vec![layer]),
    );
    p
}

fn render(r: &mut WgpuRenderer, p: &Project) {
    let mut nodes = MapRegistry::default();
    for kind in all_kinds() {
        nodes.insert(kind);
    }
    r.render_frame(&RenderRequest {
        project: p,
        composition: &Id::new("main"),
        time: RationalTime::ZERO,
        size: SIZE,
        quality: Quality::Full,
        nodes: &nodes,
    })
    .expect("描ける");
    assert!(r.take_errors().is_empty());
}

#[test]
fn デコーダーの画素の版が上がれば素材の画像を描き直す() {
    let Some(ctx) = gpu() else { return };
    let mut r = WgpuRenderer::new(
        ctx,
        Arc::new(TaggedFactory),
        Arc::new(SampleMixerFactory),
        sample_services(),
    );
    let p = project();
    VERSION_UP.store(false, Ordering::SeqCst);
    render(&mut r, &p);
    render(&mut r, &p);
    assert_eq!(
        r.frame_cache_stats().entries,
        1,
        "同じ実装の名前なら覚えた画像に当たる"
    );
    VERSION_UP.store(true, Ordering::SeqCst);
    render(&mut r, &p);
    assert_eq!(
        r.frame_cache_stats().entries,
        2,
        "版の上がった実装の画像は別のキーで描き直す"
    );
}
