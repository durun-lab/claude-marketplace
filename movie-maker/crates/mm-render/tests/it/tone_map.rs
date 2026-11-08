//! タイムラインの作業の明るさが HDR で HDR の素材があるとき、GPU の NV12 の書き出しがプレビューと同じ合成の後の
//! トーンマップを通ること。プレビューに渡る線形のフレームに `bt2390_to_sdr`（`TONEMAP_WGSL` と同じ値。
//! `mm-color-lut` の GPU の試験が確かめる）を掛けた画素と、書き出した NV12 の画素を比べる。帯の値は HDR の素材の
//! フィクスチャ（`mm_testkit::hdr`）の線形の値。作業の明るさが SDR の入力の側のトーンマップは `input_tone.rs`。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use crate::common;

use common::{comp, gpu, layer, project, registry, render_at, renderer, seconds, shader};
use mm_audio_api::AudioBlock;
use mm_decode_api::{EncodeReport, EncodeSettings, Encoder, Nv12Frame};
use mm_doc::{Asset, Project, WorkingRange};
use mm_node_api::Quality;
use mm_render_api::{tone_map_peak, RangeRequest, Renderer};
use mm_testkit::fixtures::SAMPLE_ASSET_PATH;
use mm_testkit::hdr::{self, HdrKind};
use mm_types::tonemap::bt2390_to_sdr;
use mm_types::transfer::linear_to_bt1886;
use mm_types::{ColorSpace, Id, Rational, RationalTime, Result, RgbaImage, Size, TimeRange};

/// 帯 1 つの幅（画素）。色差の 2x2 が帯をまたがない偶数。
const BAND: u32 = 16;
pub(crate) const SIZE: Size = Size::new(BAND * hdr::BANDS.len() as u32, 8);
/// NV12 の符号語の許容。f16 の中間と GPU の `pow` の丸めの分。
pub(crate) const CODE_TOLERANCE: i32 = 1;

/// PQ の素材のフィクスチャの帯の線形の値を、横に並べて出すシェーダー。
fn bands_shader() -> String {
    let arms: String = (0..hdr::BANDS.len())
        .map(|band| {
            let [r, g, b] = hdr::expected(HdrKind::Pq, band);
            format!("if (i == {band}) {{ return vec4<f32>({r:?}, {g:?}, {b:?}, 1.0); }}\n")
        })
        .collect();
    format!(
        "fn mm_main(uv: vec2<f32>) -> vec4<f32> {{\n    let i = i32(floor(uv.x * {}.0));\n{arms}    return vec4<f32>(0.0, 0.0, 0.0, 1.0);\n}}\n",
        hdr::BANDS.len()
    )
}

/// フィクスチャの帯を描くコンポジション `main`。`hdr` なら PQ の素材を足す（置かないが、素材の上限の明るさが上がる）。
fn bands_project(hdr: bool) -> Project {
    shader_project(&bands_shader(), hdr)
}

/// WGSL の `mm_main` を全面に描く、作業の明るさが HDR のコンポジション `main`。`hdr` なら PQ の素材を置かずに足す。
fn shader_project(wgsl: &str, hdr: bool) -> Project {
    let mut p = project(
        vec![comp("main", SIZE, 1, vec![layer("fill", shader(wgsl), 1)])],
        vec![],
    );
    p.color_config.working_range = WorkingRange::Hdr;
    if hdr {
        p.assets.push(pq_asset());
    }
    p
}

/// PQ の素材（`SampleDecoderFactory` が開く）。
pub(crate) fn pq_asset() -> Asset {
    Asset {
        id: Id::new("pq"),
        path: SAMPLE_ASSET_PATH.into(),
        content_hash: "pq00000000000000".into(),
        color_space: ColorSpace::bt2100_pq(),
        duration: seconds(1),
        fps: Some(Rational::integer(30)),
        timebase_offset: RationalTime::ZERO,
    }
}

/// NV12 を受けて溜めるエンコーダー。
#[derive(Default)]
struct Nv12Capture(Vec<Nv12Frame>);

impl Encoder for Nv12Capture {
    fn begin(&mut self, _settings: &EncodeSettings) -> Result<()> {
        Ok(())
    }

    fn push_frames(&mut self, _frames: &[RgbaImage]) -> Result<()> {
        panic!("NV12 の経路で書き出す")
    }

    fn push_nv12_frames(&mut self, frames: &[Nv12Frame]) -> Result<()> {
        self.0.extend_from_slice(frames);
        Ok(())
    }

    fn push_audio(&mut self, _block: &AudioBlock) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<EncodeReport> {
        Ok(EncodeReport {
            frames: self.0.len() as u64,
            audio_frames: 0,
            bytes_written: 0,
        })
    }
}

/// 線形の RGB を SDR のトーンマップ、Rec.1886、Rec.709 の tv レンジの (Y, Cb, Cr) の符号語にする（一様な帯の値）。
pub(crate) fn expected_yuv(rgb: [f32; 3], peak: f32) -> [i32; 3] {
    let e = bt2390_to_sdr(rgb, peak).map(|c| linear_to_bt1886(c.clamp(0.0, 1.0)));
    let y = 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
    let (cb, cr) = ((e[2] - y) / 1.8556, (e[0] - y) / 1.5748);
    [16.0 + 219.0 * y, 128.0 + 224.0 * cb, 128.0 + 224.0 * cr].map(|v| v.round() as i32)
}

/// 帯 `band` の中央の (Y, Cb, Cr) の符号語。
fn yuv_at(frame: &Nv12Frame, band: u32) -> [i32; 3] {
    let (x, y) = (band * BAND + BAND / 2, SIZE.height / 2);
    let w = SIZE.width as usize;
    let luma = frame.y[y as usize * w + x as usize];
    let c = (y as usize / 2) * w + (x as usize / 2) * 2;
    [luma, frame.uv[c], frame.uv[c + 1]].map(i32::from)
}

pub(crate) fn export_nv12(r: &mut mm_render::WgpuRenderer, p: &Project) -> Nv12Frame {
    let nodes = registry();
    let id = Id::new("main");
    let fps = Rational::integer(30);
    let req = RangeRequest {
        project: p,
        composition: &id,
        range: TimeRange::new(
            RationalTime::ZERO,
            RationalTime::from_frames(1, fps).unwrap(),
        )
        .unwrap(),
        fps,
        size: SIZE,
        quality: Quality::Full,
        nodes: &nodes,
    };
    let mut enc = Nv12Capture::default();
    r.export_range(&req, &mut enc, 1, &mut |_| {})
        .expect("書き出せる");
    enc.0.pop().expect("1 フレーム")
}

#[test]
fn hdr_の素材があると_nv12_の書き出しはプレビューと同じトーンマップを通る() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    for hdr_source in [true, false] {
        let p = bands_project(hdr_source);
        let peak = tone_map_peak(&p);
        assert_eq!(
            peak > 1.0,
            hdr_source,
            "素材の上限の明るさは HDR の素材があるときだけ 1 を超える"
        );
        let preview = render_at(&mut r, &p, "main", RationalTime::ZERO, SIZE);
        let exported = export_nv12(&mut r, &p);
        for band in 0..hdr::BANDS.len() as u32 {
            let (x, y) = (band * BAND + BAND / 2, SIZE.height / 2);
            let px = preview.pixel(x, y);
            let want = expected_yuv([px.r, px.g, px.b], peak);
            let got = yuv_at(&exported, band);
            let close = got
                .iter()
                .zip(want)
                .all(|(g, w)| (g - w).abs() <= CODE_TOLERANCE);
            assert!(
                close,
                "HDR の素材 {hdr_source} の帯 {band}: {got:?} 期待 {want:?}"
            );
        }
    }
}

/// プレビューに渡る線形のフレーム（フレームのキャッシュに入る画像）は、トーンマップの前の値のまま。
/// 変換は書き出しの符号化と表示の直前だけで掛かるので、キャッシュのキーに変換を入れなくてよい。
#[test]
fn 描いたフレームはトーンマップの前の線形の値で_hdr_の素材の有無で変わらない() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let with = render_at(
        &mut r,
        &bands_project(true),
        "main",
        RationalTime::ZERO,
        SIZE,
    );
    let without = render_at(
        &mut r,
        &bands_project(false),
        "main",
        RationalTime::ZERO,
        SIZE,
    );
    assert_eq!(with.max_diff(&without).expect("同じ大きさ"), 0.0);
    let top = with.pixel(2 * BAND + BAND / 2, SIZE.height / 2);
    assert!(
        top.r > 4.0,
        "PQ の 1000 cd/m² の付近の帯は 1 を超える: {top:?}"
    );
}

/// 左に SDR の白（線形 1.0。文字や画像の白）、右に HDR のハイライト（線形 2.0、406 cd/m²）。
const WHITE_AND_HIGHLIGHT: &str = "fn mm_main(uv: vec2<f32>) -> vec4<f32> {
    if (uv.x < 0.5) { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }
    return vec4<f32>(2.0, 2.0, 2.0, 1.0);
}
";

/// 書き出した Y の 8 bit の符号化値（左の 1/4 と右の 1/4 の中央）。
pub(crate) fn white_and_highlight_luma(frame: &Nv12Frame) -> (i32, i32) {
    let row = (SIZE.height / 2 * SIZE.width) as usize;
    let at = |x: u32| i32::from(frame.y[row + x as usize]);
    (at(SIZE.width / 4), at(SIZE.width * 3 / 4))
}

/// 作業の明るさが HDR のとき、HDR の素材が無ければ SDR の白は 235 のままで、ハイライトも白に切る。HDR の素材があると
/// 白は肩の中で 214 に下がり、ハイライトは白と 235 の間に別の明るさで残る。引き換えは `03-data-model.md` の「色」。
#[test]
fn sdr_の白は_hdr_の素材が無ければ_235_あれば_214_でハイライトは白の上に残る() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let sdr = export_nv12(&mut r, &shader_project(WHITE_AND_HIGHLIGHT, false));
    assert_eq!(white_and_highlight_luma(&sdr), (235, 235));
    let hdr = export_nv12(&mut r, &shader_project(WHITE_AND_HIGHLIGHT, true));
    let (white, highlight) = white_and_highlight_luma(&hdr);
    assert!((white - 214).abs() <= CODE_TOLERANCE, "白 {white}");
    assert!(
        (highlight - 231).abs() <= CODE_TOLERANCE && highlight < 235,
        "ハイライト {highlight}"
    );
}
