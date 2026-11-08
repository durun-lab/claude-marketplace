//! 子プロセスの起動。
//!
//! コンソールを持たない親（GUI サブシステムのアプリなど）からコンソールのプログラムを起動すると、
//! Windows は子ごとに新しいコンソールのウィンドウを開く。そのときだけ `CREATE_NO_WINDOW` を付け、
//! 子には見えないコンソールを持たせる。子が開く GUI のウィンドウ（CEF など）と、継承した標準入出力には影響しない。
//! 親がコンソールを持つとき（端末から動かした `mm` など）は付けない。付けると子が親のコンソールから外れ、
//! Ctrl+C が子に届かなくなる。Git for Windows の `mingw_spawnve_fd` と同じ判定。

use std::ffi::OsStr;
use std::process::Command;

/// Windows の `CREATE_NO_WINDOW`。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `program` を起動する [`Command`]。親がコンソールを持たなければ、子のコンソールのウィンドウを出さない。
///
/// このリポジトリでは `std::process::Command::new` の代わりにこれを使う（`scripts/check-conventions.sh` が確かめる）。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(creation_flags(has_console()));
    }
    cmd
}

/// 親のコンソールの有無から、子に付けるフラグを決める。
#[cfg(windows)]
fn creation_flags(parent_has_console: bool) -> u32 {
    if parent_has_console {
        0
    } else {
        CREATE_NO_WINDOW
    }
}

/// このプロセスがコンソールにつながっているか。標準出力を振り向けていても、`CONOUT$` はつながったコンソールを開く。
/// 起動のたびに確かめ、後から `AllocConsole` したプロセスにも合わせる。
#[cfg(windows)]
fn has_console() -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open("CONOUT$")
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 起動した子の出力を受け取れる() {
        let exe = std::env::current_exe().expect("テストの実行ファイル");
        let out = command(exe).arg("--list").output().expect("起動できる");
        assert!(out.status.success());
        assert!(!out.stdout.is_empty());
    }

    #[cfg(windows)]
    mod windows {
        use std::os::windows::process::CommandExt;

        use super::super::*;

        /// コンソールを持たない子にする Windows の `DETACHED_PROCESS`。
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        /// 子の役のテストに、自分のコンソールの有無を書かせる環境変数。
        const PROBE_ENV: &str = "MM_PROCESS_PROBE";

        #[test]
        fn 親のコンソールの有無でフラグを決める() {
            assert_eq!(creation_flags(true), 0);
            assert_eq!(creation_flags(false), CREATE_NO_WINDOW);
        }

        /// 子の役。`PROBE_ENV` があるときだけ、自分がコンソールにつながっているかを書く。
        #[test]
        fn probe_child_helper() {
            if std::env::var_os(PROBE_ENV).is_some() {
                println!("has_console={}", has_console());
            }
        }

        /// `flags` を付けて子の役を起動し、子が書いたコンソールの有無を返す。
        fn child_has_console(flags: u32) -> bool {
            let exe = std::env::current_exe().expect("テストの実行ファイル");
            let out = Command::new(exe)
                .args(["--exact", "process::tests::windows::probe_child_helper"])
                .arg("--nocapture")
                .env(PROBE_ENV, "1")
                .creation_flags(flags)
                .output()
                .expect("起動できる");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("has_console="), "{stdout}");
            stdout.contains("has_console=true")
        }

        #[test]
        fn コンソールの有無を見分ける() {
            assert!(!child_has_console(DETACHED_PROCESS));
            assert!(child_has_console(CREATE_NO_WINDOW));
        }
    }
}
