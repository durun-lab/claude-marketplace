//! 列挙（`describe`）と、`Prepare` の後に共有メモリで受け取る記述。

use std::path::Path;

use mm_host_clap::describe_bundle;

use crate::{plugin_path, start, DELAY, EXE, GAIN, P_GAIN, TIMEOUT};

#[test]
fn describe_はファイルの全部のプラグインとパラメータを返す() {
    let desc = describe_bundle(Path::new(EXE), &plugin_path(), TIMEOUT).expect("describe");
    let ids: Vec<_> = desc.plugins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, [GAIN, DELAY]);
    assert!(desc.skipped.is_empty(), "{:?}", desc.skipped);
    let gain = &desc.plugins[0];
    assert_eq!(gain.name, "MM Test Gain");
    assert_eq!((gain.input_channels, gain.output_channels), (2, 2));
    assert!(gain.is_effect());
    assert!(gain.features.iter().any(|f| f == "audio-effect"));
    let p = gain.params.iter().find(|p| p.id == P_GAIN).expect("gain");
    assert_eq!(
        (p.name.as_str(), p.min, p.max, p.default),
        ("gain", 0.0, 4.0, 1.0)
    );
    assert!(!p.stepped);
    assert!(desc.plugins[1].params.is_empty());
}

#[test]
fn 読めないファイルは起動の失敗で返る() {
    let err =
        describe_bundle(Path::new(EXE), Path::new("無い.clap"), TIMEOUT).expect_err("失敗するはず");
    assert!(matches!(err, mm_types::MmError::HostCrashed(_)), "{err:?}");
}

#[test]
fn 読み込んだプラグインの記述を共有メモリで受け取る() {
    let effect = start(GAIN);
    assert_eq!(effect.desc().id, GAIN);
    assert_eq!(effect.desc().params.len(), 3);
}
