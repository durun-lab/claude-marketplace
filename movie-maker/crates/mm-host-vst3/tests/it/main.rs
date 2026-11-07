//! `mm-host-vst3` を子プロセスで起こし、テストのプラグイン（`tests/plugins/mm_vst3_test.rs` を `cargo test` が
//! 例として作る DLL を、バンドルの形に置いたもの）で、列挙、処理、パラメータの位置、分け方に依らない結果、
//! クラッシュと時間切れからの回復、種類 `audio.vst3` を確かめる。

mod describe;
mod kind;
mod perf;
mod process;
mod recover;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use mm_audio_api::AudioBlock;
use mm_host_api::HostSpec;
use mm_host_vst3::{ParamPoint, Vst3Effect};

pub const EXE: &str = env!("CARGO_BIN_EXE_mm-host-vst3");
pub const RATE: u32 = 48_000;
/// テストのプラグインのクラスの ID（`tests/plugins/mm_vst3_test.rs`）。
pub const GAIN: &str = "4D4D544553544741494E000000000001";
pub const DELAY: &str = "4D4D5445535444454C41590000000002";
/// テストのプラグインのパラメータの id。値は正規化した値（gain は 4 倍、sleep は 20 秒倍）。
pub const P_GAIN: u32 = 1;
pub const P_CRASH: u32 = 2;
pub const P_SLEEP: u32 = 3;
/// gain の正規化した値 1 が表す倍率。
pub const GAIN_MAX: f64 = 4.0;
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// 例の DLL の名前。
const PLUGIN_DLL: &str = "mm_vst3_test.dll";
/// バンドルの名前。
const BUNDLE: &str = "MmTest.vst3";

/// テストのプラグインのバンドル（`<tmp>/MmTest.vst3/Contents/x86_64-win/MmTest.vst3`）。
/// 読み込まれている DLL は上書きできないので、試験のプロセスごとに置き場所を分ける。
pub fn plugin_path() -> PathBuf {
    static BUNDLE_PATH: OnceLock<PathBuf> = OnceLock::new();
    BUNDLE_PATH
        .get_or_init(|| {
            let exe = std::env::current_exe().expect("試験の実行ファイル");
            let profile = exe.parent().and_then(|d| d.parent()).expect("target の下");
            let dll = profile.join("examples").join(PLUGIN_DLL);
            assert!(
                dll.is_file(),
                "{} が無い。`cargo nextest run -p mm-host-vst3`（例も作る）で走らせる",
                dll.display()
            );
            let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("mm-vst3-{}", std::process::id()));
            let bundle = root.join(BUNDLE);
            let inner = bundle.join("Contents").join("x86_64-win");
            std::fs::create_dir_all(&inner).expect("バンドルのフォルダー");
            std::fs::copy(&dll, inner.join(BUNDLE)).expect("DLL を写す");
            bundle
        })
        .clone()
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

pub fn start(plugin: &str) -> Vst3Effect {
    Vst3Effect::start(spec(TIMEOUT), plugin_path(), Some(plugin.into()), RATE).expect("起動")
}

/// 倍率から gain の正規化した値。
pub fn gain(factor: f64) -> f64 {
    factor / GAIN_MAX
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
