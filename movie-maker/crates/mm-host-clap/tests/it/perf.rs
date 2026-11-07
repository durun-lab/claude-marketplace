//! 共有メモリの往復の遅れ。README の数字はこの試験の出力（`--no-capture`）から取る。

use std::time::{Duration, Instant};

use crate::{at_start, signal, start, GAIN};

/// 測る回数。
const ROUNDS: usize = 2000;
/// 1 回の往復の中央値の上限。48 kHz の 128 フレーム（2.67 ms）に十分収まる値。
const P50_LIMIT: Duration = Duration::from_millis(1);

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

#[test]
fn 往復の遅れはブロックの長さより十分短い() {
    let mut effect = start(GAIN);
    for frames in [128, 512, 4096] {
        let block = signal(frames, 0);
        let points = at_start(&[]);
        let mut times = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let mut b = block.clone();
            let t = Instant::now();
            effect.process(&mut b, &points).expect("処理");
            times.push(t.elapsed());
        }
        times.sort();
        let (p50, p99) = (percentile(&times, 0.5), percentile(&times, 0.99));
        let budget = Duration::from_secs_f64(frames as f64 / 48_000.0);
        eprintln!("clap.roundtrip frames={frames} p50={p50:?} p99={p99:?} budget={budget:?}");
        assert!(p50 < P50_LIMIT, "{frames} フレームの中央値 {p50:?}");
    }
}
