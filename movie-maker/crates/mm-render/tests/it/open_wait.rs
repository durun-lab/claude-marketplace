//! 素材を開くのを待つ上限（アプリのプレビュー）。開くのが上限を超えたら、開き終わるのを待たずにそのレイヤーを除いて描き、
//! 開き終わった後のフレームで描く。入れ子の中のレイヤーを除いた画像はキャッシュに覚えず、開き終わった後に描き直す。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mm_decode_api::{Decoder, DecoderFactory, GpuContextRef};
use mm_doc::{Asset, Composition, Layer, Node, Project};
use mm_node_api::Quality;
use mm_render::{GpuContext, PendingSources, WgpuRenderer};
use mm_render_api::{RenderRequest, Renderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::{SampleDecoderFactory, SampleMedia};
use mm_testkit::fixtures::SAMPLE_ASSET_PATH;
use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime, Result, RgbaImage, Size};

use crate::common::{gpu, registry, services};

const SIZE: Size = Size::new(64, 36);
/// f16 の丸めの許容。
const TOLERANCE: f32 = 3e-3;
/// 1 フレームで素材を開くのを待つ上限。
const WAIT: Duration = Duration::from_millis(20);
/// 開くのを止めている間の 1 フレームの描画に許す長さ。止めずに待てば、ここを超えて返らない。
const FRAME_LIMIT: Duration = Duration::from_secs(5);

/// 試験が許すまで開くのを止めるデコーダー。長い録画の素材の、PTS の索引を作る間の代わり。
struct Gated(Mutex<Receiver<()>>);

impl DecoderFactory for Gated {
    fn id(&self) -> &'static str {
        "gated"
    }

    fn open(&self, path: &Path, gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        let _ = self.0.lock().expect("gate").recv();
        SampleDecoderFactory::default().open(path, gpu)
    }
}

fn comp(id: &str, layers: Vec<Layer>) -> Composition {
    let fps = Rational::integer(30);
    Composition::new(
        Id::new(id),
        SIZE.width,
        SIZE.height,
        fps,
        RationalTime::seconds(1),
    )
    .with_layers(layers)
}

fn layer(id: &str, node: Node) -> Layer {
    let zero = RationalTime::ZERO;
    Layer::new(id, node, zero, zero, RationalTime::seconds(1))
}

/// 素材を画布の全体に置いたコンポジション `main`。`nested` なら素材を入れ子のコンポジション `inner` の中に置く。
fn project(nested: bool) -> Project {
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
    if nested {
        let inner =
            Node::new("composition", 1).with_param("composition", ParamValue::Text("inner".into()));
        p.compositions
            .push(comp("main", vec![layer("inner", inner)]));
        p.compositions
            .push(comp("inner", vec![layer("clip", footage)]));
    } else {
        p.compositions
            .push(comp("main", vec![layer("clip", footage)]));
    }
    p
}

fn renderer(ctx: &Arc<GpuContext>, decoders: Arc<dyn DecoderFactory>) -> WgpuRenderer {
    WgpuRenderer::new(
        ctx.clone(),
        decoders,
        Arc::new(SampleMixerFactory),
        services(),
    )
}

fn render(r: &mut WgpuRenderer, p: &Project) -> Arc<RgbaImage> {
    let nodes = registry();
    let frame = r
        .render_frame(&RenderRequest {
            project: p,
            composition: &Id::new("main"),
            time: RationalTime::ZERO,
            size: SIZE,
            quality: Quality::Full,
            nodes: &nodes,
        })
        .expect("描ける");
    assert!(r.take_errors().is_empty());
    r.gpu().download(&frame).expect("リードバックできる")
}

fn wait_opened(pending: &PendingSources, n: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while pending.opened() < n {
        assert!(Instant::now() < deadline, "素材を開き終わらない");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 開くのを止めたデコーダーのレンダラーと、開くのを許す送り手。
fn gated(ctx: &Arc<GpuContext>) -> (WgpuRenderer, Sender<()>) {
    let (release, gate) = mpsc::channel();
    let r = renderer(ctx, Arc::new(Gated(Mutex::new(gate)))).with_open_wait(Some(WAIT));
    (r, release)
}

#[test]
fn 開くのが待つ上限を超えた素材のレイヤーは除いて描き_開き終わった後のフレームで描く() {
    let Some(ctx) = gpu() else { return };
    for nested in [false, true] {
        let p = project(nested);
        let want = render(
            &mut renderer(&ctx, Arc::new(SampleDecoderFactory::default())),
            &p,
        );
        let (mut r, release) = gated(&ctx);
        let pending = r.pending_sources();
        let started = Instant::now();
        let first = render(&mut r, &p);
        let took = started.elapsed();
        assert!(
            took < FRAME_LIMIT,
            "入れ子 {nested}: 開き終わるのを待った {took:?}"
        );
        assert_eq!(
            pending.layers(),
            1,
            "入れ子 {nested}: 描かなかったレイヤーを数える"
        );
        assert!(
            first.data().iter().all(|v| *v == 0.0),
            "入れ子 {nested}: 開いている途中のレイヤーは透明"
        );

        release.send(()).expect("開くのを許す");
        wait_opened(&pending, 1);
        let second = render(&mut r, &p);
        assert_eq!(pending.layers(), 0, "入れ子 {nested}");
        let diff = second.max_diff(&want).expect("同じ大きさ");
        assert!(
            diff <= TOLERANCE,
            "入れ子 {nested}: 開いた後も除いたまま（差 {diff}）"
        );
    }
}

#[test]
fn 待つ上限を決めないレンダラーは開き終わるまで待って描く() {
    let Some(ctx) = gpu() else { return };
    let p = project(false);
    let want = render(
        &mut renderer(&ctx, Arc::new(SampleDecoderFactory::default())),
        &p,
    );
    let (release, gate) = mpsc::channel();
    let mut r = renderer(&ctx, Arc::new(Gated(Mutex::new(gate))));
    release.send(()).expect("開くのを許す");
    let got = render(&mut r, &p);
    assert_eq!(r.pending_sources().layers(), 0);
    assert!(got.max_diff(&want).expect("同じ大きさ") <= TOLERANCE);
}
