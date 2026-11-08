//! 子プロセスの起こし方。
//!
//! 画面のアプリ（GUI サブシステム）や、コンソールを持たない親から FFmpeg や各ホストを起こすと、
//! Windows は子ごとにコンソールのウィンドウを開く。`CREATE_NO_WINDOW` はコンソールを作らないだけで、
//! 子が自分で開く GUI のウィンドウ（CEF など）と、継承した標準入出力のハンドルには影響しない。

use std::ffi::OsStr;
use std::process::Command;

/// Windows の `CREATE_NO_WINDOW`。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// コンソールのウィンドウを出さない設定を足す。Windows 以外では何もしない。
pub fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `program` を起こす [`Command`]。コンソールのウィンドウを出さない。
///
/// このリポジトリでは `std::process::Command::new` の代わりにこれを使う（`scripts/check-conventions.sh` が確かめる）。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    hidden(&mut cmd);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 起こした子の出力を受け取れる() {
        let exe = std::env::current_exe().expect("試験の実行ファイル");
        let out = command(exe).arg("--list").output().expect("起動できる");
        assert!(out.status.success());
        assert!(!out.stdout.is_empty());
    }
}
