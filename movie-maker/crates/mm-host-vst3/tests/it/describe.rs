//! 列挙（`describe`）と、`Prepare` の後に共有メモリで受け取る記述。

use std::path::Path;

use mm_host_vst3::describe_bundle;

use crate::{plugin_path, start, DELAY, EXE, GAIN, P_CRASH, P_GAIN, TIMEOUT};

#[test]
fn describe_はバンドルの_audio_module_class_とパラメータを返す() {
    let desc = describe_bundle(Path::new(EXE), &plugin_path(), TIMEOUT).expect("describe");
    let ids: Vec<_> = desc.plugins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, [GAIN, DELAY], "編集のクラスは入らない");
    assert!(desc.skipped.is_empty(), "{:?}", desc.skipped);
    let gain = &desc.plugins[0];
    assert_eq!(
        (gain.name.as_str(), gain.vendor.as_str()),
        ("MM Test Gain", "durun-lab")
    );
    assert_eq!((gain.input_channels, gain.output_channels), (2, 2));
    assert!(gain.is_effect());
    assert_eq!(gain.features, ["Fx"]);
    let p = gain.params.iter().find(|p| p.id == P_GAIN).expect("gain");
    assert_eq!(
        (p.name.as_str(), p.min, p.max, p.default),
        ("gain", 0.0, 1.0, 0.25),
        "値は正規化した値"
    );
    assert!(!p.stepped);
    let crash = gain.params.iter().find(|p| p.id == P_CRASH).expect("crash");
    assert!(crash.stepped);
    let delay = &desc.plugins[1];
    assert_eq!(delay.features, ["Fx", "Delay"]);
    assert!(delay.params.is_empty(), "別の編集のクラスを作ってつなぐ");
}

#[test]
fn 読めないファイルは起動の失敗で返る() {
    let err =
        describe_bundle(Path::new(EXE), Path::new("無い.vst3"), TIMEOUT).expect_err("失敗するはず");
    assert!(matches!(err, mm_types::MmError::HostCrashed(_)), "{err:?}");
}

#[test]
fn 読み込んだプラグインの記述を共有メモリで受け取る() {
    let effect = start(GAIN);
    assert_eq!(effect.desc().id, GAIN);
    assert_eq!(effect.desc().params.len(), 3);
}
