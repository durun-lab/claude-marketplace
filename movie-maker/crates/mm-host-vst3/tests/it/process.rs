//! 音のブロックの処理。gain の値、パラメータの効く位置、遅延の続き、分け方に依らない結果。

use std::collections::BTreeMap;

use mm_audio_api::AudioBlock;
use mm_host_vst3::{ParamPoint, Vst3Effect};

use crate::{at_start, gain, signal, start, DELAY, GAIN, P_GAIN, RATE};

#[test]
fn gain_は全部のサンプルに掛かる() {
    let mut effect = start(GAIN);
    let input = signal(1000, 0);
    let mut block = input.clone();
    effect
        .process(&mut block, &at_start(&[(P_GAIN, gain(0.5))]))
        .expect("処理");
    for (o, i) in block.samples.iter().zip(&input.samples) {
        assert_eq!(*o, i * 0.5);
    }
}

#[test]
fn パラメータを載せなければ既定値で処理する() {
    let mut effect = start(GAIN);
    let input = signal(256, 3);
    let mut block = input.clone();
    effect.process(&mut block, &at_start(&[])).expect("処理");
    assert_eq!(block, input, "gain の既定は 1");
}

#[test]
fn パラメータはその位置のサンプルから効く() {
    let mut effect = start(GAIN);
    let input = signal(300, 1);
    let mut block = input.clone();
    let points = vec![
        ParamPoint {
            offset: 0,
            values: BTreeMap::from([(P_GAIN, gain(1.0))]),
        },
        ParamPoint {
            offset: 100,
            values: BTreeMap::from([(P_GAIN, gain(2.0))]),
        },
    ];
    effect.process(&mut block, &points).expect("処理");
    let (before, after) = block.samples.split_at(200);
    assert_eq!(before, &input.samples[..200]);
    for (o, i) in after.iter().zip(&input.samples[200..]) {
        assert_eq!(*o, i * 2.0);
    }
}

#[test]
fn 範囲の外の値は範囲に収めて送る() {
    let mut effect = start(GAIN);
    let input = signal(64, 2);
    let mut block = input.clone();
    effect
        .process(&mut block, &at_start(&[(P_GAIN, 100.0)]))
        .expect("処理");
    assert_eq!(
        block.samples[0],
        input.samples[0] * 4.0,
        "正規化した値の上限は 1"
    );
}

#[test]
fn 遅延はブロックの切れ目をまたいで続く() {
    let mut effect = start(DELAY);
    let input = signal(200, 0);
    let mut a = AudioBlock::from_samples(RATE, 2, input.samples[..200].to_vec()).unwrap();
    let mut b = AudioBlock::from_samples(RATE, 2, input.samples[200..].to_vec()).unwrap();
    effect.process(&mut a, &at_start(&[])).expect("前半");
    effect.process(&mut b, &at_start(&[])).expect("後半");
    assert_eq!(&a.samples[..2], &[0.0, 0.0], "最初のフレームは 0");
    assert_eq!(&a.samples[2..], &input.samples[..198]);
    assert_eq!(&b.samples[..], &input.samples[198..398]);
}

#[test]
fn reset_は遅延の中身を消す() {
    let mut effect = start(DELAY);
    let mut a = signal(10, 0);
    effect.process(&mut a, &at_start(&[])).expect("処理");
    effect.reset().expect("reset");
    let mut b = signal(10, 5);
    effect.process(&mut b, &at_start(&[])).expect("処理");
    assert_eq!(&b.samples[..2], &[0.0, 0.0]);
}

/// 全体の長さで 64 フレームごとに gain を変える点の列のうち、`[start, start + len)` の分（区間の先頭からの位置）。
fn ramp_points(start: usize, len: usize) -> Vec<ParamPoint> {
    let first = start / 64 * 64;
    (first..start + len)
        .step_by(64)
        .map(|f| ParamPoint {
            offset: f.saturating_sub(start),
            values: BTreeMap::from([(P_GAIN, gain((f / 64 % 7) as f64 * 0.25))]),
        })
        .collect()
}

fn run_split(effect: &mut Vst3Effect, input: &AudioBlock, step: usize) -> Vec<f32> {
    let frames = input.frames();
    let mut out = Vec::new();
    let mut start = 0;
    while start < frames {
        let len = step.min(frames - start);
        let samples = input.samples[start * 2..(start + len) * 2].to_vec();
        let mut block = AudioBlock::from_samples(RATE, 2, samples).unwrap();
        effect
            .process(&mut block, &ramp_points(start, len))
            .expect("処理");
        out.extend(block.samples);
        start += len;
    }
    out
}

#[test]
fn 分け方に依らず同じサンプルになる() {
    let input = signal(10_000, 0);
    for plugin in [GAIN, DELAY] {
        let whole = run_split(&mut start(plugin), &input, input.frames());
        for step in [1, 333, 4096, 5000] {
            let split = run_split(&mut start(plugin), &input, step);
            assert!(whole == split, "{plugin} を {step} フレームずつ");
        }
    }
}

#[test]
fn ステレオでないブロックは断る() {
    let mut effect = start(GAIN);
    let mut mono = AudioBlock::silence(RATE, 1, 16);
    assert!(effect.process(&mut mono, &at_start(&[])).is_err());
    let mut other_rate = AudioBlock::silence(44_100, 2, 16);
    assert!(effect.process(&mut other_rate, &at_start(&[])).is_err());
}
