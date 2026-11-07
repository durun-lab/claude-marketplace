//! アプリ側のランチャー。`mm-host-cef serve` を `mm_host_pipe::PipeHost` で起こす。
//!
//! CEF はキャッシュとページを置く一時フォルダーを要り、子のプロセスが `libcef.dll` を見つけられる `PATH` を要る。
//! 期限切れの応答の共有ハンドルはこのプロセスへ複製されたものなので、読み捨てるときに閉じる。

use std::process::Command;
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostSpec};
use mm_host_pipe::{PipeHost, PipeKind, StaleHandles};
use mm_types::Result;

/// ホストへ一時フォルダーを渡す環境変数。ホストはこの下に CEF のキャッシュとページを置く。
pub const WORK_DIR_ENV: &str = "MM_HOST_WORK_DIR";
/// CEF の配布物の置き場所。子プロセスが `libcef.dll` を見つけられるよう `PATH` の先頭に足す。
pub const CEF_PATH_ENV: &str = "CEF_PATH";
/// 起動直後の `Ping` の期限。CEF の初期化と GPU プロセスの起動を含む。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// `mm-host-cef` の起こし方。
const KIND: PipeKind = PipeKind {
    name: "host_cef",
    startup_timeout: STARTUP_TIMEOUT,
    work_dir_env: Some(WORK_DIR_ENV),
    stale_handles: StaleHandles::Close,
    configure: set_child_path,
};

/// `mm-host-cef` を起こすランチャー。
#[derive(Debug, Clone, Default)]
pub struct CefLauncher;

impl CefLauncher {
    /// 起動して `Ping` が通るまで待つ。一時フォルダーを見るテストのために具体の型で返す。
    pub fn start(spec: HostSpec) -> Result<PipeHost> {
        PipeHost::start(KIND, spec)
    }
}

impl HostLauncher for CefLauncher {
    fn id(&self) -> &'static str {
        "cef"
    }

    fn spawn(&self, spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        Ok(Box::new(Self::start(spec.clone())?))
    }
}

fn set_child_path(cmd: &mut Command) {
    if let Some(path) = child_path() {
        cmd.env("PATH", path);
    }
}

/// 子プロセスの `PATH`。`CEF_PATH` があれば先頭に足し、子プロセスが `libcef.dll` を見つけられるようにする。
pub(crate) fn child_path() -> Option<std::ffi::OsString> {
    let cef = std::env::var_os(CEF_PATH_ENV)?;
    let rest = std::env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(cef.into()).chain(std::env::split_paths(&rest));
    std::env::join_paths(paths).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_types::MmError;

    #[test]
    fn 実行ファイルが無ければ起動の失敗を返す() {
        let spec = HostSpec {
            executable: "mm-host-cef-が無い場所/none.exe".into(),
            args: Vec::new(),
            default_timeout: Duration::from_secs(1),
            gpu: None,
            client_pid: None,
        };
        let err = CefLauncher.spawn(&spec).err().expect("起動できないはず");
        assert!(matches!(err, MmError::HostCrashed(_)), "{err:?}");
    }
}
