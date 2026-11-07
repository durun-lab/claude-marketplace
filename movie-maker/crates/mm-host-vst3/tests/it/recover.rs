//! プラグインのクラッシュ、時間切れ、読み込みの失敗。アプリのプロセスは巻き込まれず、次の処理で起こし直す。

use std::time::{Duration, Instant};

use mm_host_vst3::Vst3Effect;
use mm_types::MmError;

use crate::{
    at_start, gain, plugin_path, signal, spec, start, GAIN, P_CRASH, P_GAIN, P_SLEEP, RATE,
};

#[test]
fn クラッシュは_host_crashed_で返り_次の処理で起こし直す() {
    let mut effect = start(GAIN);
    let first = effect.pid().expect("pid");
    let mut block = signal(128, 0);
    let err = effect
        .process(&mut block, &at_start(&[(P_CRASH, 1.0)]))
        .expect_err("落ちるはず");
    assert!(matches!(err, MmError::HostCrashed(_)), "{err:?}");
    assert!(!effect.is_alive());

    let input = signal(128, 1);
    let mut block = input.clone();
    effect
        .process(&mut block, &at_start(&[(P_GAIN, gain(0.5))]))
        .expect("起こし直して処理");
    assert_ne!(effect.pid(), Some(first), "別のプロセス");
    for (o, i) in block.samples.iter().zip(&input.samples) {
        assert_eq!(*o, i * 0.5);
    }
}

#[test]
fn 時間切れは_timeout_で返り_次の処理で起こし直す() {
    let limit = Duration::from_millis(500);
    let mut effect =
        Vst3Effect::start(spec(limit), plugin_path(), Some(GAIN.into()), RATE).expect("起動");
    let mut block = signal(64, 0);
    let started = Instant::now();
    let err = effect
        .process(&mut block, &at_start(&[(P_SLEEP, 1.0)]))
        .expect_err("時間切れのはず");
    assert!(matches!(err, MmError::Timeout { .. }), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5), "期限で戻る");

    let input = signal(64, 1);
    let mut block = input.clone();
    effect
        .process(&mut block, &at_start(&[]))
        .expect("起こし直して処理");
    assert_eq!(block, input);
}

#[test]
fn 無いクラスと読めないバンドルは起動で失敗する() {
    let missing_class = "00000000000000000000000000000001";
    let err = Vst3Effect::start(
        spec(crate::TIMEOUT),
        plugin_path(),
        Some(missing_class.into()),
        RATE,
    )
    .err()
    .expect("失敗するはず");
    assert!(err.to_string().contains(missing_class), "{err}");
    let bad_id = Vst3Effect::start(spec(crate::TIMEOUT), plugin_path(), Some("x".into()), RATE);
    assert!(bad_id.is_err());
    let missing = Vst3Effect::start(spec(crate::TIMEOUT), "無い.vst3", None, RATE);
    assert!(missing.is_err());
}

#[test]
fn ホストを止めても次の処理で起こし直す() {
    let mut effect = start(GAIN);
    effect.kill().expect("止める");
    assert!(!effect.is_alive());
    let input = signal(32, 0);
    let mut block = input.clone();
    effect.process(&mut block, &at_start(&[])).expect("処理");
    assert!(effect.is_alive());
    assert_eq!(block, input);
}
