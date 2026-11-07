//! ホストの実行ファイルの側。サブコマンドの振り分け、標準入力の要求に答える繰り返し、共有メモリの要求を処理する繰り返し。
//!
//! 規格ごとの違いは、`describe` の関数、`Prepare` の関数、音のブロックを処理する [`Plugin`] で渡す。
//! プラグインのクラッシュはこのプロセスだけを止め、アプリには `HostCrashed` で届く。

use std::fs::File;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

use mm_host_api::{HostRequest, HostResponse, PROTOCOL_VERSION};

use super::block::{object_names, Block, Command, Event, Mapping, Wake};
use super::desc::{BundleDesc, PrepareParams};
use super::DESCRIBE_COMMAND;
use crate::SERVE_COMMAND;

/// アプリとつなぐ共有メモリとイベント。
pub struct Link {
    pub mapping: Mapping,
    pub request: Event,
    pub done: Event,
}

impl Link {
    /// アプリが作った元の名前（[`PrepareParams::link`]）で開く。
    pub fn open(link: &str) -> Result<Self, String> {
        let [mem, req, done] = object_names(link);
        Ok(Self {
            mapping: Mapping::open(&mem).map_err(|e| e.to_string())?,
            request: Event::open(&req).map_err(|e| e.to_string())?,
            done: Event::open(&done).map_err(|e| e.to_string())?,
        })
    }
}

/// 音のブロックを処理するプラグイン（音のスレッドが持つ）。
pub trait Plugin {
    /// 共有メモリの `frames` フレームとパラメータのイベントを処理し、出力を書く。
    fn process(&mut self, block: &mut Block) -> Result<(), String>;
    /// 内部の状態（遅延の中身など）を消す。パラメータの値は残す。
    fn reset(&mut self) -> Result<(), String>;
}

/// 要求のイベントを待って処理し続ける。アプリが居なくなるまで戻らない。
pub fn serve_blocks(plugin: &mut impl Plugin, link: &mut Link) {
    while link.request.wait(None, None) == Wake::Signaled {
        let block = link.mapping.block();
        let result = match block.command() {
            Some(Command::Process) => plugin.process(block),
            Some(Command::Reset) => plugin.reset(),
            None => Err("知らない要求".to_string()),
        };
        match result {
            Ok(()) => block.set_ok(),
            Err(e) => block.set_error(&e),
        }
        block.done_seq = block.seq;
        if link.done.set().is_err() {
            break;
        }
    }
}

/// 標準入力の要求に 1 行ずつ答える。`Prepare` は 1 つのホストで 1 回だけ `prepare` に渡す。
/// 標準入力が閉じるか `Shutdown` が来たら戻る。応答は `out` に書く。
pub fn serve_requests(
    name: &str,
    out: &mut impl Write,
    mut prepare: impl FnMut(PrepareParams) -> Result<(), String>,
) -> Result<(), String> {
    let mut prepared = false;
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| format!("要求を読めない: {e}"))?;
        let (response, quit) = match serde_json::from_str::<HostRequest>(&line) {
            Ok(req) => handle(name, req, &mut prepared, &mut prepare),
            Err(e) => (error(format!("要求を読めない: {e}")), false),
        };
        let text = serde_json::to_string(&response).map_err(|e| e.to_string())?;
        writeln!(out, "{text}")
            .and_then(|()| out.flush())
            .map_err(|e| format!("応答を書けない: {e}"))?;
        if quit {
            break;
        }
    }
    // 音のスレッドはプラグインの中で止まっていることがあるので、待たずにプロセスごと終える。
    std::mem::forget(prepare);
    Ok(())
}

fn error(message: String) -> HostResponse {
    HostResponse::Error { message }
}

fn handle(
    name: &str,
    req: HostRequest,
    prepared: &mut bool,
    prepare: &mut impl FnMut(PrepareParams) -> Result<(), String>,
) -> (HostResponse, bool) {
    let pong = HostResponse::Pong {
        protocol_version: PROTOCOL_VERSION,
    };
    let result = match req {
        HostRequest::Ping => Ok(pong),
        HostRequest::Shutdown => return (pong, true),
        HostRequest::Prepare { .. } if *prepared => {
            Err("Prepare は 1 つのホストで 1 回だけ。別のプラグインはホストを起こし直す".into())
        }
        HostRequest::Prepare { params } => serde_json::from_value::<PrepareParams>(params)
            .map_err(|e| format!("Prepare の params が読めない: {e}"))
            .and_then(|p| match p.sample_rate {
                0 => Err("サンプルレートは 1 以上".into()),
                _ => prepare(p),
            })
            .map(|()| {
                *prepared = true;
                HostResponse::Prepared
            }),
        HostRequest::RenderFrame { .. } => {
            Err("音のホストは画像を描かない。音は共有メモリで渡す".to_string())
        }
    };
    if let Err(e) = &result {
        eprintln!("{name}: {e}");
    }
    (result.unwrap_or_else(error), false)
}

/// ホストの実行ファイルのサブコマンド。
pub struct HostMain {
    /// 実行ファイルの名前（エラーの先頭）。
    pub name: &'static str,
    /// 引数が合わないときの文。
    pub usage: &'static str,
    /// `serve`。応答を書く先を受け取る。
    pub serve: fn(&mut File) -> Result<(), String>,
    /// `describe <ファイル>`。
    pub describe: fn(&Path) -> Result<BundleDesc, String>,
}

impl HostMain {
    /// サブコマンドを実行する。
    pub fn run(&self) -> ExitCode {
        quiet_crash_dialogs();
        let result = protocol_out().and_then(|mut out| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            match (args.first().map(String::as_str), args.get(1)) {
                (Some(SERVE_COMMAND), _) => (self.serve)(&mut out),
                (Some(DESCRIBE_COMMAND), Some(path)) => {
                    let desc = (self.describe)(Path::new(path))?;
                    write_line(&mut out, &desc)
                }
                _ => Err(self.usage.to_string()),
            }
        });
        match result {
            Ok(()) => end_without_teardown(),
            Err(e) => {
                eprintln!("{}: {e}", self.name);
                ExitCode::FAILURE
            }
        }
    }
}

/// 応答を書き終えたら、プラグインの終わりの処理（DLL の切り離し、atexit）を実行せずにプロセスを終える。
/// 解放しなかったインスタンスの終わりの処理でクラッシュするプラグインがあり、書き終えた結果まで失敗に見えるため。
#[cfg(windows)]
fn end_without_teardown() -> ExitCode {
    use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
    // SAFETY: このプロセス自身を終えるだけ。応答の出力は書くたびに flush してある。
    let _ = unsafe { TerminateProcess(GetCurrentProcess(), 0) };
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn end_without_teardown() -> ExitCode {
    ExitCode::SUCCESS
}

/// 結果の JSON を 1 行で書く。
fn write_line(out: &mut impl Write, desc: &BundleDesc) -> Result<(), String> {
    let text = serde_json::to_string(desc).map_err(|e| e.to_string())?;
    writeln!(out, "{text}")
        .and_then(|()| out.flush())
        .map_err(|e| format!("結果を書けない: {e}"))
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
