//! 別プロセス越しの 1 フレームの往復（要求を送ってから、アプリ側が共有テクスチャを開いて自分のテクスチャへ
//! コピーし終えるまで）を測って出す。時間はほかのプロセスの負荷で揺れるので、判定するテストにせず手で回す。
//!
//! ```bash
//! export CEF_PATH=D:/dev/cef/156
//! cargo build -p mm-host-cef --release --features cef
//! cargo run -p mm-host-cef --release --features cef --example bench_roundtrip [-- <ホストの実行ファイル>]
//! ```
//!
//! 実行ファイルを渡さなければ `find_host()` が探す（`MM_CEF_HOST`、無ければ例と同じフォルダーか 1 つ上）。

#[cfg(windows)]
#[path = "../tests/it/shared_texture.rs"]
mod shared_texture;

#[cfg(windows)]
mod bench {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use mm_gpu::{GpuContext, GpuRequest};
    use mm_host_api::{HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame};
    use mm_host_cef::{find_host, CefLauncher, PipeHost};
    use mm_types::{MmError, RationalTime, Result, Size};

    use crate::shared_texture::open_shared;

    const PAGE: &str = include_str!("../assets/bench.html");
    /// 1 回の要求の上限。測るだけなので、負荷の高い機材でも切れない長さにする。
    const REQUEST_LIMIT: Duration = Duration::from_secs(30);
    /// 測らない助走と、測るフレームの数。時刻は 30 fps で進める。
    const WARMUP: i64 = 30;
    const FRAMES: i64 = 300;
    const FPS: u32 = 30;
    const SIZES: [Size; 2] = [Size::new(1920, 1080), Size::new(1280, 720)];

    /// 大きさごとに往復を測り、p50 と p95 を標準出力に書く。
    pub fn run() -> Result<()> {
        let exe = match std::env::args().nth(1) {
            Some(path) => PathBuf::from(path),
            None => find_host()
                .ok_or_else(|| MmError::NotFound("CEF で描けるホストの実行ファイル".into()))?,
        };
        let ctx = GpuContext::shared(&GpuRequest {
            adapter: None,
            optional_features: wgpu::Features::empty(),
            backends: wgpu::Backends::DX12,
        })?;
        for size in SIZES {
            let mut ms = measure(&exe, &ctx, size)?;
            ms.sort_by(f64::total_cmp);
            println!(
                "往復 {}x{}: p50 {:.2} ms, p95 {:.2} ms（{FRAMES} フレーム、アプリ側の GPU のコピーの完了まで）",
                size.width,
                size.height,
                percentile(&ms, 0.5),
                percentile(&ms, 0.95)
            );
        }
        Ok(())
    }

    /// ホストを起こし、助走の後の各フレームの往復をミリ秒で返す。
    fn measure(exe: &std::path::Path, ctx: &GpuContext, size: Size) -> Result<Vec<f64>> {
        let mut host = CefLauncher::start(HostSpec {
            executable: exe.to_path_buf(),
            args: Vec::new(),
            default_timeout: REQUEST_LIMIT,
            gpu: Some(ctx.adapter.clone()),
            client_pid: Some(std::process::id()),
        })?;
        let prepare = HostRequest::Prepare {
            params: serde_json::json!({ "html": PAGE }),
        };
        host.request(&prepare, REQUEST_LIMIT)?;
        let mut ms = Vec::new();
        for n in 0..WARMUP + FRAMES {
            let start = Instant::now();
            let handle = frame(&mut host, RationalTime::new(i128::from(n), FPS)?, size)?;
            copy_owned(ctx, &open_shared(ctx, handle, size))?;
            if n >= WARMUP {
                ms.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        Ok(ms)
    }

    /// 時刻 `time` の 1 フレームを頼み、共有テクスチャのハンドルを返す。
    fn frame(host: &mut PipeHost, time: RationalTime, size: Size) -> Result<u64> {
        let req = HostRequest::RenderFrame {
            time,
            size,
            params: serde_json::json!({}),
        };
        match host.request(&req, REQUEST_LIMIT)? {
            HostResponse::Frame(SharedFrame::SharedTexture { handle, .. }) => Ok(handle),
            other => Err(MmError::Unsupported(format!(
                "共有テクスチャで返らない: {other:?}"
            ))),
        }
    }

    /// アプリが受け取ったフレームを自分のテクスチャへコピーし、完了を待つ（アプリ側の取り込みの費用）。
    fn copy_owned(ctx: &GpuContext, source: &wgpu::Texture) -> Result<wgpu::Texture> {
        let target = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("app frame"),
            size: source.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: source.format(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            target.as_image_copy(),
            source.size(),
        );
        encoder.transition_resources(
            std::iter::empty(),
            std::iter::once(wgpu::TextureTransition {
                texture: source,
                selector: None,
                state: wgpu::TextureUses::PRESENT,
            }),
        );
        ctx.queue.submit([encoder.finish()]);
        ctx.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| MmError::Internal(format!("コピーの完了待ち: {e}")))?;
        Ok(target)
    }

    fn percentile(sorted: &[f64], p: f64) -> f64 {
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }
}

#[cfg(windows)]
fn main() -> mm_types::Result<()> {
    bench::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("共有テクスチャの往復は Windows（D3D12）だけで測れる");
}
