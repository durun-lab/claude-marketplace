//! `mm-host-clap` を子プロセスで起こし、テストのプラグイン（`build.rs` が公式のヘッダーで作る `MmTest.clap`）で
//! 列挙、処理、パラメータの位置、分け方に依らない結果、クラッシュと時間切れからの回復、種類 `audio.clap` を確かめる。

mod describe;
mod kind;
mod perf;
mod process;
mod recover;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use mm_audio_api::AudioBlock;
use mm_host_api::HostSpec;
use mm_host_clap::{ClapEffect, ParamPoint};

pub const EXE: &str = env!("CARGO_BIN_EXE_mm-host-clap");
const PLUGINS: &str = env!("MM_CLAP_TEST_PLUGINS");
pub const RATE: u32 = 48_000;
pub const GAIN: &str = "jp.durun.mm.test.gain";
pub const DELAY: &str = "jp.durun.mm.test.delay";
/// テストのプラグインのパラメータの id（`tests/plugins/mm_test.c`）。
pub const P_GAIN: u32 = 1;
pub const P_CRASH: u32 = 2;
pub const P_SLEEP: u32 = 3;
pub const TIMEOUT: Duration = Duration::from_secs(5);

pub fn plugin_path() -> PathBuf {
    PathBuf::from(PLUGINS).join("MmTest.clap")
}

pub fn spec(timeout: Duration) -> HostSpec {
    HostSpec {
        executable: EXE.into(),
        args: Vec::new(),
        default_timeout: timeout,
        gpu: None,
        client_pid: None,
    }
}

pub fn start(plugin: &str) -> ClapEffect {
    ClapEffect::start(spec(TIMEOUT), plugin_path(), Some(plugin.into()), RATE).expect("起動")
}

/// 左右で違う、決まった値の列（`seed` でずらす）。
pub fn signal(frames: usize, seed: usize) -> AudioBlock {
    let samples = (0..frames * 2)
        .map(|i| {
            let n = (i + seed * 2) as f32;
            ((n * 0.013).sin() * 0.5 + (n * 0.0007).cos() * 0.25)
                * if i % 2 == 0 { 1.0 } else { -0.8 }
        })
        .collect();
    AudioBlock::from_samples(RATE, 2, samples).expect("ステレオ")
}

/// 先頭の 1 点だけのパラメータ。
pub fn at_start(values: &[(u32, f64)]) -> Vec<ParamPoint> {
    vec![ParamPoint {
        offset: 0,
        values: values.iter().copied().collect::<BTreeMap<_, _>>(),
    }]
}
