//! アプリ側のランチャー。`mm-host-ofx serve` を `mm_host_pipe::PipeHost` で起こす。
//!
//! このホストは CPU の画像だけを返すので、一時フォルダーを作らず、期限切れの応答に共有テクスチャが
//! 入っていたら閉じずに漏れとして数える。

use std::path::Path;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostSpec};
use mm_host_pipe::{PipeHost, PipeKind, StaleHandles};
use mm_types::Result;

use crate::desc::BundleDesc;

pub use mm_host_pipe::DESCRIBE_COMMAND;
/// 起動直後の `Ping` の期限。プラグインはまだ読み込まないので短くてよい。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// `mm-host-ofx` の起こし方。
const KIND: PipeKind = PipeKind {
    name: "host_ofx",
    startup_timeout: STARTUP_TIMEOUT,
    work_dir_env: None,
    stale_handles: StaleHandles::Count,
    configure: |_| {},
};

/// `mm-host-ofx` を起こすランチャー。
#[derive(Debug, Clone, Default)]
pub struct OfxLauncher;

impl OfxLauncher {
    /// 起動して `Ping` が通るまで待つ。
    pub fn start(spec: HostSpec) -> Result<PipeHost> {
        PipeHost::start(KIND, spec)
    }
}

impl HostLauncher for OfxLauncher {
    fn id(&self) -> &'static str {
        "ofx"
    }

    fn spawn(&self, spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        Ok(Box::new(Self::start(spec.clone())?))
    }
}

/// `executable describe <bundle>` を別プロセスで実行し、バンドルの記述を受け取る。
/// プラグインの describe がクラッシュしても、このプロセスには `HostCrashed` で返る。
pub fn describe_bundle(executable: &Path, bundle: &Path, timeout: Duration) -> Result<BundleDesc> {
    mm_host_pipe::run_describe(executable, bundle, timeout, KIND.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_types::MmError;

    #[test]
    fn 実行ファイルが無ければ起動の失敗を返す() {
        let spec = HostSpec {
            executable: "mm-host-ofx-が無い場所/none.exe".into(),
            args: Vec::new(),
            default_timeout: Duration::from_secs(1),
            gpu: None,
            client_pid: None,
        };
        let err = OfxLauncher.spawn(&spec).err().expect("起動できないはず");
        assert!(matches!(err, MmError::HostCrashed(_)), "{err:?}");
    }
}
