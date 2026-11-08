//! 1080p で字幕を想定した大きさ（54 px、幅 1800）の 1 行と 10 行を、CPU と glyphon（GPU）で描く時間を測って出す。
//! 既定の判断材料で、時間はほかのプロセスの負荷で揺れるので、判定するテストにせず手で回す。
//!
//! ```bash
//! cargo run -p mm-text-glyphon --release --example bench_raster
//! ```

use mm_gpu::{GpuContext, GpuRequest};
use mm_text_api::{TextAlign, TextRasterizer, TextShaper, TextStyle};
use mm_text_glyphon::TextEngine;
use mm_types::Rgba;
use std::time::Instant;

/// 初回を除いて測る 2 回目以降の回数。
const WARM_RUNS: usize = 20;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let ctx = GpuContext::shared(&GpuRequest::default()).expect("GPU を作れる");
    let engine = TextEngine::new();
    let (shaper, cpu, glyphon) = (
        engine.shaper(),
        engine.rasterizer(),
        engine.gpu_rasterizer(ctx),
    );
    let st = TextStyle {
        font_family: "Yu Gothic".into(),
        size_px: 54.0,
        max_width: Some(1800.0),
        align: TextAlign::Left,
        color: Rgba::new(0.9, 0.8, 0.3, 1.0),
        ..TextStyle::default()
    };
    let line = "動画編集ソフトの字幕テスト。Hello, movie-maker! 日本語と English の混在。";
    let one = line.to_string();
    let ten = (0..10)
        .map(|i| format!("{i}行目 {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    for (label, text) in [("1 行", one), ("10 行", ten)] {
        let shaped = shaper.shape(&text, &st).expect("シェーピング");
        let r: [(&str, &dyn TextRasterizer); 2] = [("cpu", &*cpu), ("glyphon", &*glyphon)];
        for (name, raster) in r {
            let t0 = Instant::now();
            let first = raster.rasterize(&shaped, &st, 1.0).expect("描く");
            let cold = t0.elapsed().as_secs_f64() * 1e3;
            let warm: Vec<f64> = (0..WARM_RUNS)
                .map(|_| {
                    let t = Instant::now();
                    raster.rasterize(&shaped, &st, 1.0).expect("描く");
                    t.elapsed().as_secs_f64() * 1e3
                })
                .collect();
            println!(
                "{label} {name}: 画像 {}x{}、初回 {cold:.2} ms、2 回目以降の中央値 {:.2} ms",
                first.width(),
                first.height(),
                median(warm)
            );
        }
    }
}
