//! `web.cef` で 1080p の 1 フレームを描く時間（ホストの描画、共有テクスチャの受け取り、変換、ヘルパーへの
//! コピー、GPU の完了まで）を測って出す。時間はほかのプロセスの負荷で揺れるので、判定するテストにせず手で回す。
//!
//! ```bash
//! export CEF_PATH=D:/dev/cef/156
//! cargo build -p mm-host-cef --release --features cef
//! cargo run -p mm-nodes-cef --release --example bench_cef_frame [-- <ホストの実行ファイル>]
//! ```
//!
//! 実行ファイルを渡さなければ `mm_host_cef::find_host()` が探す（`MM_CEF_HOST`、無ければ例と同じフォルダー）。

#[cfg(windows)]
mod bench {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    use mm_gpu::{GpuContext, GpuRequest, WgpuGpu};
    use mm_host_cef::{find_host, CefLauncher};
    use mm_node_api::{FrameCx, GpuHelper, NoInputs, NodeKind, Quality};
    use mm_nodes_cef::{WebCefKind, DATA_PARAM, HTML_PARAM, STATIC_PARAM};
    use mm_testkit::fixtures::sample_services;
    use mm_types::{ParamValue, Params, RationalTime, Size};

    const PAGE: &str = include_str!("../tests/it/pages/probe.html");
    const FULL_HD: Size = Size::new(1920, 1080);
    /// 測らない助走と、測るフレームの数。時刻は 30 fps で進める。
    const WARMUP: i64 = 20;
    const FRAMES: i64 = 120;
    const FPS: u32 = 30;

    fn percentile(sorted: &[f64], p: f64) -> f64 {
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }

    /// 1080p を 1 フレームずつ描いて p50 と p95、最後にキャッシュにヒットしたときの時間を標準出力に書く。
    pub fn run() {
        let exe = match std::env::args().nth(1) {
            Some(path) => PathBuf::from(path),
            None => find_host()
                .expect("CEF で描けるホストの実行ファイルが要る（CEF_PATH と MM_CEF_HOST）"),
        };
        let ctx = GpuContext::shared(&GpuRequest {
            backends: wgpu::Backends::DX12,
            ..GpuRequest::default()
        })
        .expect("D3D12 の GPU");
        let mut gpu = WgpuGpu::new(ctx.clone());
        let kind = WebCefKind::with_host(Arc::new(CefLauncher), exe);
        let mut p = Params::new();
        p.set(HTML_PARAM, ParamValue::Text(PAGE.into()))
            .set(
                DATA_PARAM,
                ParamValue::Json(serde_json::json!({"label": "bench"})),
            )
            .set(STATIC_PARAM, ParamValue::Bool(false));
        let mut frame_at = |n: i64| {
            let start = Instant::now();
            let t = RationalTime::new(i128::from(n), FPS).expect("時刻");
            let services = sample_services();
            let mut inputs = NoInputs;
            let mut cx = FrameCx {
                time: t,
                params: &p,
                size: FULL_HD,
                region: FULL_HD.to_rect(),
                scale: 1.0,
                quality: Quality::Full,
                inputs: &mut inputs,
                footage: None,
                source_time: t,
                gpu: &mut gpu,
                services: &services,
            };
            let frame = kind.render(&mut cx).expect("描ける");
            gpu.release(&frame);
            gpu.after_submit();
            ctx.device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("GPU の完了待ち");
            start.elapsed().as_secs_f64() * 1000.0
        };
        let mut ms = Vec::new();
        for n in 0..WARMUP + FRAMES {
            let took = frame_at(n);
            if n >= WARMUP {
                ms.push(took);
            }
        }
        let hit = frame_at(WARMUP + FRAMES - 1);
        ms.sort_by(f64::total_cmp);
        println!(
            "web.cef 1920x1080: p50 {:.2} ms, p95 {:.2} ms（{FRAMES} フレーム）、キャッシュにヒットしたとき {hit:.2} ms",
            percentile(&ms, 0.5),
            percentile(&ms, 0.95),
        );
    }
}

fn main() {
    #[cfg(windows)]
    bench::run();
    #[cfg(not(windows))]
    eprintln!("Windows だけで動く");
}
