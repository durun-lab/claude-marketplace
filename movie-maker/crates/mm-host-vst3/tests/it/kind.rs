//! 種類 `audio.vst3`。レンダラーと同じ形（区間、インスタンスのキー、時刻の格子のパラメータ）で呼び、
//! 書き出し（1 回で全体）と再生（短い区間の連続）が同じサンプルになること、シークで状態を消すことを確かめる。

use mm_audio_api::AudioBlock;
use mm_host_vst3::{param_key, Vst3EffectKind, KIND_ID, PLUGIN_ID_PARAM, PLUGIN_PARAM};
use mm_node_api::{AudioEffectCx, NodeKind, AUDIO_PARAM_STEP};
use mm_types::{ParamValue, Params, Rational, RationalTime, TimeRange};

use crate::{gain, plugin_path, signal, DELAY, EXE, GAIN, P_CRASH, P_GAIN, RATE, TIMEOUT};

fn kind() -> Vst3EffectKind {
    Vst3EffectKind::new(Some(EXE.into()))
        .with_timeout(TIMEOUT)
        .with_roots(vec![plugin_path()
            .parent()
            .expect("試験のプラグインのフォルダー")
            .to_path_buf()])
}

fn frame_time(frame: u64) -> RationalTime {
    RationalTime::from_frames(frame as i64, Rational::integer(RATE)).unwrap()
}

/// gain を 1 秒で 0 倍から 2 倍へ上げるキーフレームを、時刻 `frame` で評価した値。
fn params_at(plugin: &str, frame: u64, crash: bool) -> Params {
    let mut p = Params::new();
    p.set(
        PLUGIN_PARAM,
        ParamValue::Text(plugin_path().display().to_string()),
    );
    p.set(PLUGIN_ID_PARAM, ParamValue::Text(plugin.into()));
    p.set(
        param_key(P_GAIN),
        ParamValue::Number(gain(frame as f64 / RATE as f64 * 2.0)),
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
    kind: &Vst3EffectKind,
    plugin: &str,
    start: u64,
    block: &mut AudioBlock,
    crash: bool,
) -> mm_types::Result<()> {
    let frames = block.frames() as u64;
    let params = grid(plugin, start, frames, crash);
    let cx = AudioEffectCx {
        range: TimeRange::new(frame_time(start), frame_time(start + frames)).unwrap(),
        instance: "comp/layer/effect",
        params: &params,
        sample_rate: RATE,
        channels: 2,
    };
    kind.process_audio(&cx, block)
}

fn render(kind: &Vst3EffectKind, plugin: &str, input: &AudioBlock, step: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, chunk) in input.samples.chunks(step * 2).enumerate() {
        let mut block = AudioBlock::from_samples(RATE, 2, chunk.to_vec()).unwrap();
        apply(kind, plugin, (i * step) as u64, &mut block, false).expect("処理");
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
    apply(&kind, DELAY, 0, &mut a, false).expect("処理");
    let mut b = signal(100, 1);
    apply(&kind, DELAY, 100, &mut b, false).expect("続き");
    assert_ne!(
        &b.samples[..2],
        &[0.0, 0.0],
        "続きは前の区間の最後のフレームを出す"
    );
    let mut c = signal(100, 2);
    apply(&kind, DELAY, 10_000, &mut c, false).expect("シーク");
    assert_eq!(&c.samples[..2], &[0.0, 0.0], "シークの後は遅延が空");
    assert_eq!(kind.instance_count(), 1);
}

#[test]
fn クラッシュの後も同じインスタンスで処理を続ける() {
    let kind = kind();
    let mut a = signal(64, 0);
    apply(&kind, GAIN, 0, &mut a, false).expect("処理");
    let first = kind.instance_pid("comp/layer/effect");
    let mut b = signal(64, 1);
    assert!(apply(&kind, GAIN, 64, &mut b, true).is_err(), "落ちる");
    let mut c = signal(64, 2);
    apply(&kind, GAIN, 128, &mut c, false).expect("起こし直して処理");
    assert_ne!(kind.instance_pid("comp/layer/effect"), first);
}

#[test]
fn 画像には何もしない種類として振る舞う() {
    let kind = kind();
    assert_eq!(kind.id(), KIND_ID);
    assert!(kind.traits().audio && !kind.traits().pixel);
    let defaults = kind.params().defaults();
    let back = kind
        .migrate(serde_json::to_value(&defaults).unwrap(), kind.version())
        .expect("同じバージョンは恒等");
    assert_eq!(back, defaults);
}
