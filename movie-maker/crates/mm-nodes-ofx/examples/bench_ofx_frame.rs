//! `ofx` の Gain で 1080p の 1 フレームを描く時間（入力を渡し、描き、結果を受け取るまで）を測って出す。
//! 時間はほかのプロセスの負荷で揺れるので、判定するテストにせず手で回す。
//!
//! ```bash
//! cargo build -p mm-host-ofx --release
//! cargo run -p mm-nodes-ofx --release --example bench_ofx_frame
//! ```
//!
//! ホストの実行ファイルは `mm_host_ofx::find_host()` が探す（`MM_OFX_HOST`、無ければ例と同じフォルダー）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_host_ofx::{describe_bundle, find_host, OfxLauncher, TEST_PLUGINS};
use mm_nodes_ofx::{Describe, OfxKind, OfxPlugin, BUNDLE_PARAM, PLUGIN_PARAM};
use mm_testkit::node::{ConstInputs, Harness};
use mm_types::{ParamValue, Params, Rational, RationalTime, Rgba, RgbaImage, Size};

const GAIN: &str = "MmTestGain.ofx.bundle";
const GAIN_ID: &str = "mm.test.Gain";
const SIZE: Size = Size::new(1920, 1080);
const FPS: Rational = Rational::integer(30);
const GREY: Rgba = Rgba::new(0.5, 0.5, 0.5, 1.0);
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(15);
/// 最初の 1 回（起動と読み込みを含む）を除いて測るフレームの数。
const TIMED_FRAMES: usize = 10;

/// 組み立て（`mm-runtime` の `ofx.rs`）と同じ形の describe。
fn describe(exe: PathBuf) -> Describe {
    Arc::new(move |bundle: &Path| {
        let desc = describe_bundle(&exe, bundle, DESCRIBE_TIMEOUT)?;
        Ok(desc
            .plugins
            .into_iter()
            .filter(|p| p.context.is_some())
            .map(|p| OfxPlugin {
                schema: p.param_schema(),
                settable: p
                    .params
                    .iter()
                    .filter_map(|x| x.to_spec().map(|s| s.name))
                    .collect(),
                id: p.id,
            })
            .collect())
    })
}

fn main() {
    let exe = find_host().expect("mm-host-ofx が要る（cargo build -p mm-host-ofx か MM_OFX_HOST）");
    let k = OfxKind::new(
        Arc::new(OfxLauncher),
        exe.clone(),
        describe(exe),
        vec![PathBuf::from(TEST_PLUGINS)],
    );
    let mut p = Params::new();
    p.set(BUNDLE_PARAM, ParamValue::Text(GAIN.into()))
        .set(PLUGIN_PARAM, ParamValue::Text(GAIN_ID.into()))
        .set("gain", ParamValue::Number(0.5));
    let mut inputs = ConstInputs(vec![Arc::new(RgbaImage::filled(SIZE, GREY))]);
    let mut h = Harness::new(None);
    let mut times = Vec::new();
    for i in 0..=TIMED_FRAMES {
        let start = Instant::now();
        let t = RationalTime::from_frames(i as i64, FPS).expect("時刻");
        h.render(&k, &p, t, SIZE, 1.0, &mut inputs).expect("描ける");
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let first = times.remove(0);
    times.sort_by(f64::total_cmp);
    let p50 = times[times.len() / 2];
    println!("ofx 1920x1080: 最初 {first:.1} ms（起動と読み込みを含む）、以後の p50 {p50:.1} ms");
}
