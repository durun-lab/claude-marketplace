//! ホストの実行ファイルの本体。`clack-host` でプラグインを同じプロセスに読み込む。
//! このプロセスはアプリと分かれているので、プラグインのクラッシュはアプリに `HostCrashed` で届くだけで済む。

mod audio;
mod plugin;
mod serve;

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use mm_host_clap::{DESCRIBE_COMMAND, SERVE_COMMAND};

const USAGE: &str = "使い方: mm-host-clap serve | mm-host-clap describe <.clap のファイル>";

/// サブコマンドを実行する。
pub fn run() -> ExitCode {
    quiet_crash_dialogs();
    let result = protocol_out().and_then(|mut out| {
        let args: Vec<String> = std::env::args().skip(1).collect();
        match (args.first().map(String::as_str), args.get(1)) {
            (Some(SERVE_COMMAND), _) => serve::run(&mut out),
            (Some(DESCRIBE_COMMAND), Some(path)) => describe(Path::new(path), &mut out),
            _ => Err(USAGE.to_string()),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mm-host-clap: {e}");
            ExitCode::FAILURE
        }
    }
}

/// プラグインのクラッシュでエラー報告のダイアログを出さず、すぐ終わってアプリに知らせる。
#[cfg(windows)]
fn quiet_crash_dialogs() {
    use windows::Win32::System::Diagnostics::Debug::{
        SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX,
    };
    // SAFETY: このプロセスのエラーモードを変えるだけ。プラグインを読み込む前に呼ぶ。
    unsafe { SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX) };
}

#[cfg(not(windows))]
fn quiet_crash_dialogs() {}

/// 応答を書く先。標準出力を複製してから、プラグインの printf が混ざらないよう標準出力を標準エラーへ付け替える。
fn protocol_out() -> Result<File, String> {
    let out = clone_stdout().map_err(|e| format!("標準出力を複製できない: {e}"))?;
    stdout_to_stderr()?;
    Ok(out)
}

#[cfg(windows)]
fn clone_stdout() -> std::io::Result<File> {
    use std::os::windows::io::AsHandle;
    Ok(File::from(
        std::io::stdout().as_handle().try_clone_to_owned()?,
    ))
}

#[cfg(not(windows))]
fn clone_stdout() -> std::io::Result<File> {
    use std::os::fd::AsFd;
    Ok(File::from(std::io::stdout().as_fd().try_clone_to_owned()?))
}

/// C の実行時ライブラリの記述子。プラグインの printf はこの番号の 1 に書く。
const CRT_STDOUT: i32 = 1;
const CRT_STDERR: i32 = 2;

#[cfg(windows)]
extern "C" {
    fn _dup2(from: i32, to: i32) -> i32;
}

/// OS の標準出力のハンドルと、C の実行時ライブラリの記述子 1 を、標準エラーに付け替える。
#[cfg(windows)]
fn stdout_to_stderr() -> Result<(), String> {
    use windows::Win32::System::Console::{
        GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    // SAFETY: 標準のハンドルを読み替えるだけ。_dup2 は C の実行時ライブラリの記述子を複製する。
    unsafe {
        let err = GetStdHandle(STD_ERROR_HANDLE).map_err(|e| e.to_string())?;
        SetStdHandle(STD_OUTPUT_HANDLE, err).map_err(|e| e.to_string())?;
        if _dup2(CRT_STDERR, CRT_STDOUT) < 0 {
            return Err("標準出力を付け替えられない".into());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn stdout_to_stderr() -> Result<(), String> {
    let _ = (CRT_STDOUT, CRT_STDERR);
    Ok(())
}

/// `.clap` のファイルを describe し、結果の JSON を 1 行で書く。
fn describe(path: &Path, out: &mut impl Write) -> Result<(), String> {
    let desc = plugin::describe_bundle(path)?;
    let text = serde_json::to_string(&desc).map_err(|e| e.to_string())?;
    writeln!(out, "{text}")
        .and_then(|()| out.flush())
        .map_err(|e| format!("結果を書けない: {e}"))
}
