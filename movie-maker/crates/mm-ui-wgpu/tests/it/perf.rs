//! 数千のカットとキーフレームの多いレイヤーの Timeline を描く時間（測定用）。release で手で動かす:
//! `MM_GPU=5090 MM_REQUIRE_GPU=1 cargo nextest run --release -p mm-ui-wgpu --run-ignored only --no-capture`
//! 数は予算の表（docs/design/17-instrumentation.md の「操作から画面まで（wgpu のサーフェス）」）と比べる。

use std::time::{Duration, Instant};

use mm_ui_layout::{CutBar, Item, ItemKind, Peaks, PointerEvent, TimelineModel, WaveSource};
use mm_ui_wgpu::{TextStats, Ui};

use crate::support::{gpu, FORMAT};

/// sequence のカットの数と、全部のレイヤーの数（ep03 の 7502 と同じくらい）。
const CUTS: usize = 3000;
const LAYERS: usize = 7500;
/// 毎フレームにキーフレームを持つレイヤー（10 分のトラッキングの結果、30 fps で 18001 個）の数。開いたときに見える上の行に置く。
const TRACKED: usize = 3;
/// 帯の波形に使う素材の数と、1 つの素材の長さ（秒）。カットは素材を順に使い、素材の途中から切り出す。
const WAVE_ASSETS: usize = 20;
const WAVE_SECS: usize = 600;
/// 1920x1080 の論理ピクセルを表示の倍率 1.5 で描くサーフェス。
const SCALE: f32 = 1.5;
const SIZE: (u32, u32) = (2880, 1620);

/// 2 秒のカットを並べた sequence と、静止画のレイヤー。名前は全部ちがう。
/// 静止画のレイヤーは始まりと終わりの 0.5 秒にフェードのキーフレームを持ち、上の `TRACKED` 本は毎フレームに持つ。
fn model() -> TimelineModel {
    let mut seq = Item::layer(0.0, CUTS as f64 * 2.0, ItemKind::Sequence, true).with_label("main");
    seq.cuts = (0..CUTS)
        .map(|i| CutBar {
            start: i as f64 * 2.0,
            end: i as f64 * 2.0 + 2.0,
            label: format!("take-{i:04}.mp4").into(),
            wave: Some(WaveSource {
                asset: format!("take-{}", i % WAVE_ASSETS).into(),
                at: i as f64 * 2.0,
                src: (i / WAVE_ASSETS % (WAVE_SECS / 2)) as f64 * 2.0,
                speed: 1.0,
            }),
            ..CutBar::default()
        })
        .collect();
    let tracked: Vec<f64> = (0..=18_000).map(|f| f64::from(f) / 30.0).collect();
    let mut items = vec![seq];
    items.extend((1..LAYERS).map(|i| {
        if i <= TRACKED {
            return Item::layer(0.0, 600.0, ItemKind::Image, true)
                .with_label(&format!("tracked-{i}.png"))
                .with_keyframes(&tracked);
        }
        let start = (i % 500) as f64 * 12.0;
        Item::layer(start, start + 8.0, ItemKind::Image, true)
            .with_label(&format!("overlay-{i}.png"))
            .with_keyframes(&[start, start + 0.5, start + 7.5, start + 8.0])
    }));
    TimelineModel {
        duration: CUTS as f64 * 2.0,
        fps: 60.0,
        items,
        ..TimelineModel::default()
    }
}

/// 1 つのシナリオの測定。`cpu` は `Ui::render` が返るまで、`total` は GPU の完了まで（ミリ秒）。
struct Phase {
    name: &'static str,
    cpu: Vec<f64>,
    total: Vec<f64>,
    text: TextStats,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 分位点（0..=1）。
fn quantile(values: &[f64], q: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).round() as usize]
}

/// 描画の対象とその上の `Ui`。
struct Bench {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: wgpu::TextureView,
    ui: Ui,
}

impl Bench {
    /// 1 フレーム描いて、CPU の時間と GPU の完了までの時間を返す。
    fn frame(&mut self) -> (f64, f64) {
        let t0 = Instant::now();
        self.ui.render(&self.device, &self.queue, &self.target);
        let cpu = ms(t0.elapsed());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        (cpu, ms(t0.elapsed()))
    }

    /// `frames` 回、`step` で表示を動かしてから描く。
    fn phase(&mut self, name: &'static str, frames: usize, step: impl Fn(&mut Ui, usize)) -> Phase {
        let before = self.ui.text_stats();
        let (mut cpu, mut total) = (Vec::new(), Vec::new());
        for i in 0..frames {
            step(&mut self.ui, i);
            let (c, t) = self.frame();
            cpu.push(c);
            total.push(t);
        }
        let after = self.ui.text_stats();
        Phase {
            name,
            cpu,
            total,
            text: TextStats {
                shaped: after.shaped - before.shaped,
                uploads: after.uploads - before.uploads,
            },
        }
    }
}

fn bench() -> Option<Bench> {
    let (device, queue) = gpu()?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_scale_factor(SCALE);
    ui.set_model(model());
    // MM_PERF_WAVES=0 でピークを置かず、波形を出さない場合（波形を足す前と同じ描画）を測る。
    if std::env::var("MM_PERF_WAVES").as_deref() != Ok("0") {
        for a in 0..WAVE_ASSETS {
            let peaks: Vec<f32> = (0..WAVE_SECS * 100)
                .map(|i| ((i * 7 + a * 13) % 100) as f32 / 100.0)
                .collect();
            ui.set_wave(format!("take-{a}").into(), Peaks::new(100.0, 1, &peaks));
        }
    }
    ui.resize(SIZE.0, SIZE.1);
    let target = texture.create_view(&Default::default());
    Some(Bench {
        device,
        queue,
        target,
        ui,
    })
}

#[test]
#[ignore = "測定用。リリースビルドで動かす"]
fn 数千のカットの_timeline_を拡大と移動と再生で描く時間() {
    let Some(mut b) = bench() else { return };
    let (first, _) = b.frame();
    println!("最初のフレーム（フォントの読み込みを含む）: {first:.2} ms");
    let t = b.ui.layout().timeline;
    let at = (t.x + t.w / 2.0, t.y + t.h / 2.0);
    let wheel = |dy, ctrl, shift| PointerEvent::Wheel {
        x: at.0,
        y: at.1,
        dy,
        ctrl,
        shift,
    };
    let phases = [
        b.phase("全体表示", 60, |_, _| {}),
        b.phase("拡大", 40, |ui, _| {
            ui.pointer(wheel(1.0, true, false));
        }),
        b.phase("移動", 120, |ui, _| {
            ui.pointer(wheel(1.0, false, false));
        }),
        b.phase("縦のスクロール", 120, |ui, _| {
            ui.pointer(wheel(-1.0, false, true));
        }),
        b.phase("再生", 120, |ui, i| ui.set_playhead(i as f64 / 60.0)),
    ];
    println!(
        "シナリオ | CPU p50 / p99 / 最大 | GPU の完了まで p50 / p99 / 最大 | シェーピングした数 | 頂点を作った数"
    );
    for p in &phases {
        println!(
            "{} | {:.2} / {:.2} / {:.2} ms | {:.2} / {:.2} / {:.2} ms | {} | {}",
            p.name,
            quantile(&p.cpu, 0.5),
            quantile(&p.cpu, 0.99),
            quantile(&p.cpu, 1.0),
            quantile(&p.total, 0.5),
            quantile(&p.total, 0.99),
            quantile(&p.total, 1.0),
            p.text.shaped,
            p.text.uploads,
        );
    }
    let play = &phases[4];
    assert_eq!(
        play.text.shaped + play.text.uploads,
        0,
        "再生中はシェーピングせず、頂点も作らない"
    );
}
