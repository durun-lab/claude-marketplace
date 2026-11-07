//! 1080p で 1 フレームを描く時間を測る。`cargo run --release -p mm-nodes-lottie --example frame_time [Lottie の JSON]`。
//! 引数が無ければ試験のフィクスチャ。毎回違う時刻を描くので、種類の覚えには当たらない。

use std::path::PathBuf;
use std::time::Instant;

use mm_node_api::NodeKind;
use mm_nodes_lottie::{LottieKind, ANIMATION_PARAM};
use mm_testkit::node::{ConstInputs, Harness};
use mm_types::{AssetId, ParamValue, Params, RationalTime, Result, Size};

/// 測る大きさ。
const SIZE: Size = Size::new(1920, 1080);
/// 測るフレームの数（30 fps の 2 秒）。
const FRAMES: i64 = 60;
/// 時刻の分母（fps）。
const FPS: u32 = 30;

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/it/fixtures/shapes.json")
        });
    let kind = LottieKind::default();
    let mut params = Params::new();
    params.set(
        ANIMATION_PARAM,
        ParamValue::AssetRef(AssetId::new(path.to_string_lossy())),
    );
    let params = params.with_defaults(&kind.params());
    let mut h = Harness::new(None);
    let mut inputs = ConstInputs(Vec::new());
    // 1 回目は読み込みと画布の確保を含むので測らない。
    h.render(&kind, &params, RationalTime::ZERO, SIZE, 1.0, &mut inputs)?;
    let start = Instant::now();
    for i in 1..=FRAMES {
        let t = RationalTime::new(i128::from(i), FPS)?;
        h.render(&kind, &params, t, SIZE, 1.0, &mut inputs)?;
    }
    let per_frame = start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64;
    println!(
        "{}: {per_frame:.2} ms / frame ({}x{})",
        path.display(),
        SIZE.width,
        SIZE.height
    );
    Ok(())
}
