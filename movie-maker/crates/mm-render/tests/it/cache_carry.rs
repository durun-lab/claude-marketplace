//! 退避先（ディスクのキャッシュ）を持ち越した次の起動で、古い画像を引かないこと。素材のファイルの書き換えと、
//! デコーダーの画素の作り方のバージョンの上がりを、同じ退避先を持つ別々のレンダラーの実際の描画で確かめる。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_audio_api::AudioBlock;
use mm_cache_api::CacheStore;
use mm_decode_api::{DecodedFrame, Decoder, DecoderFactory, GpuContextRef, MediaInfo};
use mm_doc::{Asset, Composition, Layer, Node, Project};
use mm_render::{FrameCacheConfig, GpuContext, WgpuRenderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::cache::MemoryCache;
use mm_testkit::decode::{SampleDecoderFactory, SampleMedia};
use mm_testkit::fixtures::{invert_lut_asset, INVERT_LUT_ID, SAMPLE_ASSET_PATH};
use mm_types::{
    ColorSpace, Id, ParamValue, RationalTime, Result, Rgba, RgbaImage, Size, TimeRange,
};

use crate::common::{comp, effect, gpu, layer, project, render_at, services, solid};

const SIZE: Size = Size::new(64, 36);
/// 画像 2 枚分。入れ子の中のレイヤーと入れ子を置き、`other` の 2 枚で両方を追い出す。
const TWO_IMAGES: u64 = 2 * (SIZE.width * SIZE.height) as u64 * 8;
/// 恒等の 2 点の 3D LUT。R が最も速く変わる。
const IDENTITY_CUBE: &str =
    "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
/// 古い画像と見なす差の下限。
const STALE_DIFF: f32 = 0.1;

fn t0() -> RationalTime {
    RationalTime::ZERO
}

fn renderer(
    ctx: &Arc<GpuContext>,
    decoders: Arc<dyn DecoderFactory>,
    store: Option<&Arc<dyn CacheStore>>,
) -> WgpuRenderer {
    let config = FrameCacheConfig {
        memory_bytes: if store.is_some() { TWO_IMAGES } else { 0 },
        store: store.cloned(),
    };
    WgpuRenderer::new(
        ctx.clone(),
        decoders,
        Arc::new(SampleMixerFactory),
        services(),
    )
    .with_frame_cache(config)
}

/// `inner` を入れ子にした `main` と、追い出すための単色を 2 枚重ねた `other`。
fn with_nested(inner: Vec<Layer>) -> Project {
    let nested =
        Node::new("composition", 1).with_param("composition", ParamValue::Text("inner".into()));
    let other = vec![
        layer("o1", solid(Rgba::new(1.0, 0.0, 0.0, 1.0)), 2),
        layer("o2", solid(Rgba::new(0.0, 1.0, 0.0, 0.5)), 2),
    ];
    let comps: Vec<Composition> = vec![
        comp("inner", SIZE, 2, inner),
        comp("main", SIZE, 2, vec![layer("n", nested, 2)]),
        comp("other", SIZE, 2, other),
    ];
    project(comps, Vec::new())
}

/// `main` を当たるまで描き、`other` で追い出して退避先に書き出す。最初の画像を返す。
fn render_and_spill(r: &mut WgpuRenderer, p: &Project) -> Arc<RgbaImage> {
    let first = render_at(r, p, "main", t0(), SIZE);
    render_at(r, p, "main", t0(), SIZE);
    render_at(r, p, "main", t0(), SIZE);
    render_at(r, p, "other", t0(), SIZE);
    // 追い出した画像は次のフレームの前に退避するので、もう 1 回描く。
    render_at(r, p, "other", t0(), SIZE);
    r.wait_frame_cache_spills();
    first
}

/// 新しい起動の 1 回目の `main`。入れ子が退避先から当たったか（中のレイヤーを引かなかったか）も返す。
/// 退避先から当たった画像はメモの外れとして数えるので、引いた回数で見分ける。
fn first_main(r: &mut WgpuRenderer, p: &Project) -> (Arc<RgbaImage>, bool) {
    let image = render_at(r, p, "main", t0(), SIZE);
    (image, r.frame_cache_stats().lookups == 1)
}

fn diff(a: &RgbaImage, b: &RgbaImage) -> f32 {
    a.max_diff(b).expect("同じ大きさ")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mm-render-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("一時フォルダー");
    dir
}

/// 単色に `look` の LUT を掛けた入れ子。LUT の素材のパスは `lut`。
fn lut_project(lut: &Path) -> Project {
    let node =
        Node::new("color.lut", 1).with_param("lut", ParamValue::AssetRef(Id::new(INVERT_LUT_ID)));
    let mut inner = layer("s", solid(Rgba::new(0.2, 0.4, 0.6, 1.0)), 2);
    inner.effects.push(effect("fx", node));
    let mut p = with_nested(vec![inner]);
    p.assets.push(Asset {
        path: lut.to_string_lossy().into_owned(),
        ..invert_lut_asset()
    });
    p
}

#[test]
fn 効果の素材のファイルを書き換えたら前の起動の入れ子の画像を引かない() {
    let Some(ctx) = gpu() else { return };
    let lut = temp_dir("lut-carry").join("look.cube");
    std::fs::copy(invert_lut_asset().path, &lut).expect("コピーできる");
    let p = lut_project(&lut);
    let store: Arc<dyn CacheStore> = Arc::new(MemoryCache::new(None));
    let sample = || -> Arc<dyn DecoderFactory> { Arc::new(SampleDecoderFactory::default()) };
    let inverted = render_and_spill(&mut renderer(&ctx, sample(), Some(&store)), &p);

    std::fs::write(&lut, std::fs::read(&lut).expect("読める")).expect("書ける");
    let (same, hit) = first_main(&mut renderer(&ctx, sample(), Some(&store)), &p);
    assert!(
        hit,
        "中身を変えずに書き直しただけなら前の起動の画像に当たる"
    );
    assert!(diff(&same, &inverted) <= 1e-6);

    std::fs::write(&lut, IDENTITY_CUBE).expect("書ける");
    let (fresh, hit) = first_main(&mut renderer(&ctx, sample(), Some(&store)), &p);
    assert!(!hit, "中身を書き換えた LUT の入れ子は別のキー");
    let truth = render_at(&mut renderer(&ctx, sample(), None), &p, "main", t0(), SIZE);
    assert!(diff(&fresh, &truth) <= 1e-6, "書き換えた LUT で描く");
    assert!(
        diff(&fresh, &inverted) > STALE_DIFF,
        "反転した古い画像ではない"
    );
}

/// 実装の名前。素材は 8 bit で、画素の作り方のバージョンの付く広い信号の名前は名乗らない。
const PLAIN: &str = "tagged";

/// モックのデコーダーに名前を付けたもの。`wide` が `Some` なら、新しいビルドで広い信号と判定された素材として
/// その名前を名乗り、作り方の変わった画素（単色）を返す。
struct Tagged {
    inner: Box<dyn Decoder>,
    wide: Option<&'static str>,
}

impl Decoder for Tagged {
    fn info(&self) -> &MediaInfo {
        self.inner.info()
    }

    fn seek(&mut self, t: RationalTime) -> Result<()> {
        self.inner.seek(t)
    }

    fn next_frame(&mut self) -> Result<Option<DecodedFrame>> {
        let mut frame = self.inner.next_frame()?;
        if let (Some(f), Some(_)) = (frame.as_mut(), self.wide) {
            f.image = f
                .image
                .take()
                .map(|img| RgbaImage::filled(img.size(), Rgba::new(1.0, 0.0, 1.0, 1.0)));
        }
        Ok(frame)
    }

    fn decode_audio(&mut self, range: TimeRange) -> Result<AudioBlock> {
        self.inner.decode_audio(range)
    }

    fn backend_id(&self) -> Option<&'static str> {
        Some(self.wide.unwrap_or(PLAIN))
    }
}

/// 名前が実装の名前と同じファクトリー（`auto` を通さずに 1 つの実装を選んだ設定）。`wide_id` は広い信号の名前で、
/// `wide` が真なら素材を広い信号と判定する。
struct TaggedFactory {
    wide_id: &'static str,
    wide: bool,
}

impl DecoderFactory for TaggedFactory {
    fn id(&self) -> &'static str {
        PLAIN
    }

    fn backend_ids(&self) -> Vec<&'static str> {
        vec![PLAIN, self.wide_id]
    }

    fn open(&self, path: &Path, _gpu: Option<&dyn GpuContextRef>) -> Result<Box<dyn Decoder>> {
        Ok(Box::new(Tagged {
            inner: SampleDecoderFactory::default().open(path, None)?,
            wide: self.wide.then_some(self.wide_id),
        }))
    }
}

/// 素材を画布の全体に置いた入れ子。
fn footage_project() -> Project {
    let media = SampleMedia::default();
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(Id::new("clip")));
    let mut p = with_nested(vec![layer("clip-layer", footage, 1)]);
    p.assets.push(Asset {
        id: Id::new("clip"),
        path: SAMPLE_ASSET_PATH.into(),
        content_hash: "sample0000000000".into(),
        color_space: ColorSpace::bt709(),
        duration: media.duration,
        fps: Some(media.fps),
        timebase_offset: RationalTime::ZERO,
    });
    p
}

#[test]
fn デコーダーの画素の作り方のバージョンが上がったら前の起動の入れ子の画像を引かない() {
    let Some(ctx) = gpu() else { return };
    let p = footage_project();
    let store: Arc<dyn CacheStore> = Arc::new(MemoryCache::new(None));
    let old = || -> Arc<dyn DecoderFactory> {
        Arc::new(TaggedFactory {
            wide_id: "tagged+wide.1",
            wide: false,
        })
    };
    let before = render_and_spill(&mut renderer(&ctx, old(), Some(&store)), &p);

    let mut same_build = renderer(&ctx, old(), Some(&store));
    render_at(&mut same_build, &p, "main", t0(), SIZE);
    let opened = same_build.frame_cache_stats();
    let again = render_at(&mut same_build, &p, "main", t0(), SIZE);
    let stats = same_build.frame_cache_stats();
    assert_eq!(
        stats.lookups - opened.lookups,
        1,
        "同じビルドなら、素材を開いた後の入れ子は前の起動の画像に当たる（中のレイヤーを引かない）"
    );
    assert!(diff(&again, &before) <= 1e-6);

    let new = || -> Arc<dyn DecoderFactory> {
        Arc::new(TaggedFactory {
            wide_id: "tagged+wide.2",
            wide: true,
        })
    };
    let (after, hit) = first_main(&mut renderer(&ctx, new(), Some(&store)), &p);
    assert!(!hit, "まだ開いていない素材のキーにバージョンの組が入る");
    let truth = render_at(&mut renderer(&ctx, new(), None), &p, "main", t0(), SIZE);
    assert!(diff(&after, &truth) <= 1e-6, "新しい作り方の画素で描く");
    assert!(
        diff(&after, &before) > STALE_DIFF,
        "前のビルドの画像ではない"
    );
}
