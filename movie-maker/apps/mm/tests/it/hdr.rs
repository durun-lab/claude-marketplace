//! HDR の通し: HEVC Main10 の PQ と HLG の素材を読み、SDR の白の単色を半分の不透明度で重ね（1 を超える値の合成）、
//! HEVC Main10 の PQ で書き出し、書き出したものを読み直して値を確かめる。デコーダーは組み立ての既定（`auto`）。
//! タイムラインの作業の明るさは HDR（素材をトーンマップせずに合成する）。

use crate::common;

use std::path::Path;

use mm_decode_api::DecoderFactory;
use mm_decode_ffmpeg::FfmpegDecoderFactory;
use mm_doc::{Asset, Composition, Layer, Node, Param, Project, RenderTarget, WorkingRange};
use mm_export::{export_run, ExportOptions, ExportOutcome};
use mm_render_api::RenderQuality;
use mm_runtime::{assemble, RuntimeConfig};
use mm_testkit::hdr::{self, HdrKind};
use mm_types::tonemap::{bt2390_to_sdr, hdr_source_peak};
use mm_types::{Id, ParamValue, Rational, RationalTime, Rgba};

/// 重ねる白の不透明度。
const OVERLAY: f32 = 0.5;

fn project(asset: &Path, kind: HdrKind, out: &Path) -> Project {
    let fps = Rational::integer(hdr::FPS);
    let duration = RationalTime::from_frames(i64::from(hdr::FRAMES), fps).unwrap();
    let asset: Asset = serde_json::from_value(serde_json::json!({
        "id": "hdr", "path": asset.to_string_lossy(), "content_hash": format!("hdr-{}", kind.trc()),
        "color_space": kind.color_space(), "duration": duration, "fps": fps,
    }))
    .unwrap();
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(asset.id.clone()));
    let white =
        Node::new("solid", 1).with_param("color", ParamValue::Color(Rgba::new(1.0, 1.0, 1.0, 1.0)));
    let mut overlay = Layer::new(
        "white",
        white,
        RationalTime::ZERO,
        RationalTime::ZERO,
        duration,
    );
    overlay.opacity = Param::constant(ParamValue::Number(f64::from(OVERLAY)));
    let layers = vec![
        Layer::new(
            "clip",
            footage,
            RationalTime::ZERO,
            RationalTime::ZERO,
            duration,
        ),
        overlay,
    ];
    let comp = Composition::new(
        Id::new("main"),
        hdr::SIZE.width,
        hdr::SIZE.height,
        fps,
        duration,
    )
    .with_layers(layers);
    let target: RenderTarget = serde_json::from_value(serde_json::json!({
        "id": "pq", "composition": "main", "width": hdr::SIZE.width, "height": hdr::SIZE.height,
        "fps": fps, "codec": "hevc_pq", "output_path": out.to_string_lossy(),
    }))
    .unwrap();
    let mut p = Project::new();
    // HDR の書き出しで 1 を超える値を残すので、作業の明るさを HDR にする。
    p.color_config.working_range = WorkingRange::Hdr;
    p.assets.push(asset);
    p.compositions.push(comp);
    p.render_targets.push(target);
    p
}

#[test]
fn hdr_の素材に_sdr_の白を重ねて_pq_で書き出し読み直すと値が戻る() {
    let dir = tempfile::tempdir().unwrap();
    for kind in [HdrKind::Pq, HdrKind::Hlg] {
        // 素材の ID が同じ別のプロジェクトなので、デコーダーのプールを持ち越さないよう Engine を作り直す
        let mut engine = assemble(&RuntimeConfig::real()).unwrap();
        let source = hdr::fixture(&common::fixtures(), kind);
        let out = dir.path().join(format!("{}.mp4", kind.trc()));
        let p = project(&source, kind, &out);
        let options = ExportOptions {
            target: Some(Id::new("pq")),
            ..ExportOptions::default()
        };
        let outcome = export_run(&mut engine, &p, &options).unwrap();
        assert!(
            matches!(outcome, ExportOutcome::Completed(_)),
            "{kind:?} を書き出せた"
        );

        let mut dec = FfmpegDecoderFactory.open(&out, None).unwrap();
        assert_eq!(dec.info().color_space.0, "bt2100_pq");
        let image = dec.next_frame().unwrap().unwrap().image.unwrap();
        let mut worst = 0.0f32;
        for band in 0..hdr::BANDS.len() {
            let (x, y) = hdr::band_center(band);
            let p = image.pixel(x, y);
            let want = hdr::expected(kind, band).map(|c| c * (1.0 - OVERLAY) + OVERLAY);
            for (got, want) in [p.r, p.g, p.b].into_iter().zip(want) {
                assert!(
                    (got - want).abs() <= hdr::tolerance(want),
                    "{kind:?} の帯 {band}: {got} 期待 {want}"
                );
                worst = worst.max((got - want).abs() / want.abs().max(1.0));
            }
        }
        eprintln!("{kind:?} の素材 → 合成 → PQ → 読み直しの最大の相対の差 {worst:e}");
    }
}

/// 作業の明るさが SDR（既定）なら、HDR の素材はデコードの直後に SDR のトーンマップで白（1.0）以下に収まってから
/// SDR の白と合成される。帯の値は素材の期待の値に `bt2390_to_sdr` を掛けて白を半分重ねたもので、どれも 1 以下。
#[test]
fn 作業の明るさが_sdr_なら_hdr_の素材は合成の前に白以下に収まる() {
    let dir = tempfile::tempdir().unwrap();
    for kind in [HdrKind::Pq, HdrKind::Hlg] {
        let mut engine = assemble(&RuntimeConfig::real()).unwrap();
        let source = hdr::fixture(&common::fixtures(), kind);
        let mut p = project(&source, kind, &dir.path().join("unused.mp4"));
        p.color_config.working_range = WorkingRange::Sdr;
        let peak = hdr_source_peak([&p.assets[0].color_space]);
        let image = engine
            .render_frame(
                &p,
                &Id::new("main"),
                RationalTime::ZERO,
                hdr::SIZE,
                RenderQuality::Full,
            )
            .unwrap();
        let mut bands = Vec::new();
        for band in 0..hdr::BANDS.len() {
            let (x, y) = hdr::band_center(band);
            let px = image.pixel(x, y);
            let want = bt2390_to_sdr(hdr::expected(kind, band), peak)
                .map(|c| c * (1.0 - OVERLAY) + OVERLAY);
            for (got, want) in [px.r, px.g, px.b].into_iter().zip(want) {
                assert!(
                    (got - want).abs() <= hdr::tolerance(want) && got <= 1.0 + 1e-3,
                    "{kind:?} の帯 {band}: {got} 期待 {want}"
                );
            }
            bands.push(px.r);
        }
        assert!(
            bands[0] < bands[1] && bands[1] < bands[2],
            "{kind:?}: 白の付近と白より明るい帯は別の明るさで残る {bands:?}"
        );
    }
}
