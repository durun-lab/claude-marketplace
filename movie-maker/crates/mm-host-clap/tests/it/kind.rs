//! 種類 `audio.clap`。レンダラーと同じ形（区間、インスタンスのキー、時刻の格子のパラメータ）で呼び、
//! 書き出し（1 回で全体）と再生（短い区間の連続）が同じサンプルになること、シークで状態を消すことを確かめる。

use mm_audio_api::AudioBlock;
use mm_host_clap::{param_key, ClapEffectKind, PLUGIN_ID_PARAM, PLUGIN_PARAM};
use mm_node_api::{AudioEffectCx, NodeKind, AUDIO_PARAM_STEP};
use mm_types::{ParamValue, Params, Rational, RationalTime, TimeRange};

use crate::{plugin_path, signal, DELAY, EXE, GAIN, P_CRASH, P_GAIN, RATE, TIMEOUT};

fn kind() -> ClapEffectKind {
    ClapEffectKind::new(Some(EXE.into())).with_timeout(TIMEOUT)
}

fn frame_time(frame: u64) -> RationalTime {
    RationalTime::from_frames(frame as i64, Rational::integer(RATE)).unwrap()
}

/// gain を 1 秒で 0 から 2 へ上げるキーフレームを、時刻 `frame` で評価した値。
fn params_at(plugin: &str, frame: u64, crash: bool) -> Params {
    let mut p = Params::new();
    p.set(
        PLUGIN_PARAM,
        ParamValue::Text(plugin_path().display().to_string()),
    );
    p.set(PLUGIN_ID_PARAM, ParamValue::Text(plugin.into()));
    p.set(
        param_key(P_GAIN),
        ParamValue::Number(frame as f64 / RATE as f64 * 2.0),
    );
    p.set(
        param_key(P_CRASH),
        ParamValue::Number(if crash { 1.0 } else { 0.0 }),
    );
    p
}

/// レンダラーと同じ格子の点。先頭はその前の格子の時刻で評価する。
fn grid(plugin: &str, start: u64, frames: u64, crash: bool) -> Vec<(usize, Params)> {
    let first = start / AUDIO_PARAM_STEP * AUDIO_PARAM_STEP;
    (first..start + frames)
        .step_by(AUDIO_PARAM_STEP as usize)
        .map(|f| ((f.max(start) - start) as usize, params_at(plugin, f, crash)))
        .collect()
}

fn apply(
    kind: &ClapEffectKind,
    plugin: &str,
    start: u64,
    block: &mut AudioBlock,
) -> mm_types::Result<()> {
    let frames = block.frames() as u64;
    let params = grid(plugin, start, frames, false);
    let cx = AudioEffectCx {
        range: TimeRange::new(frame_time(start), frame_time(start + frames)).unwrap(),
        instance: "comp/layer/effect",
        params: &params,
        sample_rate: RATE,
        channels: 2,
    };
    kind.process_audio(&cx, block)
}

fn render(kind: &ClapEffectKind, plugin: &str, input: &AudioBlock, step: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, chunk) in input.samples.chunks(step * 2).enumerate() {
        let mut block = AudioBlock::from_samples(RATE, 2, chunk.to_vec()).unwrap();
        apply(kind, plugin, (i * step) as u64, &mut block).expect("処理");
        out.extend(block.samples);
    }
    out
}

#[test]
fn 書き出しと再生は同じサンプルになる() {
    let input = signal(RATE as usize, 0);
    for plugin in [GAIN, DELAY] {
        let export = render(&kind(), plugin, &input, input.frames());
        for step in [480, 1024, 1601] {
            let preview = render(&kind(), plugin, &input, step);
            assert!(export == preview, "{plugin} を {step} フレームずつ");
        }
    }
}

#[test]
fn 続きでない区間ではプラグインの状態を消す() {
    let kind = kind();
    let mut a = signal(100, 0);
    apply(&kind, DELAY, 0, &mut a).expect("処理");
    let mut b = signal(100, 1);
    apply(&kind, DELAY, 100, &mut b).expect("続き");
    assert_ne!(
        &b.samples[..2],
        &[0.0, 0.0],
        "続きは前の区間の最後のフレームを出す"
    );
    let mut c = signal(100, 2);
    apply(&kind, DELAY, 10_000, &mut c).expect("シーク");
    assert_eq!(&c.samples[..2], &[0.0, 0.0], "シークの後は遅延が空");
    assert_eq!(kind.instance_count(), 1);
}

#[test]
fn クラッシュの後も同じインスタンスで処理を続ける() {
    let kind = kind();
    let mut a = signal(64, 0);
    apply(&kind, GAIN, 0, &mut a).expect("処理");
    let first = kind.instance_pid("comp/layer/effect");
    let params = grid(GAIN, 64, 64, true);
    let cx = AudioEffectCx {
        range: TimeRange::new(frame_time(64), frame_time(128)).unwrap(),
        instance: "comp/layer/effect",
        params: &params,
        sample_rate: RATE,
        channels: 2,
    };
    let mut b = signal(64, 1);
    assert!(kind.process_audio(&cx, &mut b).is_err(), "落ちる");
    let mut c = signal(64, 2);
    apply(&kind, GAIN, 128, &mut c).expect("起こし直して処理");
    assert_ne!(kind.instance_pid("comp/layer/effect"), first);
}

#[test]
fn 画像には何もしない種類として振る舞う() {
    let kind = kind();
    assert!(kind.traits().audio && !kind.traits().pixel);
    let defaults = kind.params().defaults();
    let back = kind
        .migrate(serde_json::to_value(&defaults).unwrap(), kind.version())
        .expect("同じバージョンは恒等");
    assert_eq!(back, defaults);
}
