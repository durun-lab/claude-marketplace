//! 書き出しの途中でデバイスの消失を知らされても、先行していた古いデバイスのリードバックを待ち続けず、
//! エラーで返ること。共有のコンテキストを差し替えるので、ほかのテストと同じプロセスに置かない（別の実行ファイル。docs/ci.md）。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use mm_decode_api::{EncodeReport, EncodeSettings, Encoder, Nv12Frame};
use mm_doc::{Composition, Layer, Node, Project};
use mm_node_api::Quality;
use mm_nodes_basic::all_kinds;
use mm_render::{GpuContext, GpuRequest, WgpuRenderer};
use mm_render_api::{RangeRequest, Renderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::SampleDecoderFactory;
use mm_testkit::fixtures::sample_services;
use mm_testkit::node::MapRegistry;
use mm_types::{Id, ParamValue, Rational, RationalTime, Rgba, Size, TimeRange};

const SIZE: Size = Size::new(64, 36);
const FRAMES: i64 = 12;
/// 1 つの工程（消失を知らせるまで、知らせてから書き出しが返るまで、レンダラーの片付け）で、
/// 待ち続けているとみなす秒数。
const HANG_LIMIT: Duration = Duration::from_secs(30);

/// 消失させるコンテキストをプロセスの終わりまで保持し、破棄しない。`WGPU_BACKEND=vulkan` で回したときの
/// 守りで、NVIDIA の Vulkan ドライバーは破棄の後も自分のスレッドが解放済みの領域に触れることがあり、
/// プロセスの終了と重なると 0xc0000409 でクラッシュする（gfx-rs/wgpu#6571）。既定の D3D12 では起きない。
static RETIRED: OnceLock<Arc<GpuContext>> = OnceLock::new();

/// NV12 を受け、`lose_after` 枚目を受けた時点でデバイスの消失を知らせ、`lost` にも送る。
struct LosingEncoder {
    ctx: Arc<GpuContext>,
    pushed: usize,
    lose_after: usize,
    lost: mpsc::Sender<()>,
}

impl Encoder for LosingEncoder {
    fn begin(&mut self, _settings: &EncodeSettings) -> mm_types::Result<()> {
        Ok(())
    }

    fn push_frames(&mut self, _frames: &[mm_types::RgbaImage]) -> mm_types::Result<()> {
        Ok(())
    }

    fn push_nv12_frames(&mut self, frames: &[Nv12Frame]) -> mm_types::Result<()> {
        self.pushed += frames.len();
        if self.pushed == self.lose_after {
            self.ctx.mark_lost();
            let _ = self.lost.send(());
        }
        Ok(())
    }

    fn push_audio(&mut self, _block: &mm_audio_api::AudioBlock) -> mm_types::Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> mm_types::Result<EncodeReport> {
        Ok(EncodeReport {
            frames: self.pushed as u64,
            audio_frames: 0,
            bytes_written: 0,
        })
    }
}

fn project() -> Project {
    let layer = Layer::new(
        "a",
        Node::new("solid", 1).with_param("color", ParamValue::Color(Rgba::WHITE)),
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(2),
    );
    let mut p = Project::new();
    p.compositions.push(
        Composition::new(
            Id::new("main"),
            SIZE.width,
            SIZE.height,
            Rational::integer(30),
            RationalTime::seconds(2),
        )
        .with_layers(vec![layer]),
    );
    p
}

#[test]
fn 書き出しの途中のデバイス消失は待ち続けずエラーで返る() {
    let ctx = match GpuContext::shared(&GpuRequest::default()) {
        Ok(ctx) => ctx,
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(e) => {
            eprintln!("GPU が無いのでスキップする: {e}");
            return;
        }
    };
    let ctx = RETIRED.get_or_init(|| ctx).clone();
    let (lost_tx, lost_rx) = mpsc::channel();
    let (returned_tx, returned_rx) = mpsc::channel();
    let (cleaned_tx, cleaned_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        export_losing_device(ctx, lost_tx, &returned_tx);
        let _ = cleaned_tx.send(());
    });
    // 時計は工程ごとに測り、書き出しが返るまでの時計は消失を知らせた時点から始める。
    wait(&lost_rx, "消失を知らせる所まで書き出しが進まない");
    let (is_err, recoveries) = wait(&returned_rx, "古いデバイスのリードバックを待ち続けている");
    // レンダラーと作り直したコンテキストを片付け終えてから抜け、ドライバーの破棄とプロセスの終了を重ねない。
    // 片付けを知らせた後のスレッドは返るだけなので、join は待たない。
    wait(&cleaned_rx, "レンダラーの片付けが終わらない");
    worker.join().expect("書き出しのスレッド");
    assert_eq!(recoveries, 1, "消失を見つけて作り直す");
    assert!(is_err, "先行していた古いリードバックは捨ててエラーで返す");
}

/// 2 枚目で消失を知らせる書き出しを走らせ、返った結果（エラーか、作り直した回数）を `returned` に送る。
/// レンダラーはこの関数を抜けるときに破棄する。
fn export_losing_device(
    ctx: Arc<GpuContext>,
    lost: mpsc::Sender<()>,
    returned: &mpsc::Sender<(bool, u32)>,
) {
    let mut nodes = MapRegistry::default();
    for kind in all_kinds() {
        nodes.insert(kind);
    }
    let project = project();
    let comp = Id::new("main");
    let fps = Rational::integer(30);
    let mut r = WgpuRenderer::new(
        ctx.clone(),
        Arc::new(SampleDecoderFactory::default()),
        Arc::new(SampleMixerFactory),
        sample_services(),
    );
    let mut encoder = LosingEncoder {
        ctx,
        pushed: 0,
        lose_after: 2,
        lost,
    };
    let range = TimeRange::new(
        RationalTime::ZERO,
        RationalTime::from_frames(FRAMES, fps).expect("表せる"),
    )
    .expect("範囲");
    let request = RangeRequest {
        project: &project,
        composition: &comp,
        range,
        fps,
        size: SIZE,
        quality: Quality::Full,
        nodes: &nodes,
    };
    let result = r.export_range(&request, &mut encoder, 1, &mut |_| {});
    let _ = returned.send((result.is_err(), r.recoveries()));
}

/// 書き出しのスレッドからの知らせを `HANG_LIMIT` まで待つ。スレッドが先にパニックしたら、その旨で失敗する。
fn wait<T>(rx: &mpsc::Receiver<T>, what: &str) -> T {
    match rx.recv_timeout(HANG_LIMIT) {
        Ok(v) => v,
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("{what}（{HANG_LIMIT:?} を超えた）"),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{what}（書き出しのスレッドがパニックした）")
        }
    }
}
