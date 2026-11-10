//! 同じデスクトップのほかの movie-maker のウィンドウと、前面のウィンドウを持つプロセスを調べる。再生テストのロックはふだんの起動を
//! 待たないので、ユーザーが開いている movie-maker が前面やマウスのキャプチャーを取ると工程が失敗しうる。その手がかりを
//! 結果の JSON に残す。

use serde_json::{json, Value};
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, TRUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
};

const EXE_NAME: &str = "movie-maker.exe";

/// 動いているプロセスの ID と実行ファイルの名前。取れなければ `None`。
fn processes() -> Option<Vec<(u32, String)>> {
    // SAFETY: スナップショットはこの関数の出口で閉じ、エントリーは `dwSize` を入れてから渡す。
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut list = Vec::new();
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let name = &entry.szExeFile;
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            list.push((entry.th32ProcessID, String::from_utf16_lossy(&name[..len])));
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        Some(list)
    }
}

/// `EnumWindows` のコールバック。見えているウィンドウなら、持っているプロセスの ID を `lparam` の `Vec` に足す。
unsafe extern "system" fn push_visible_owner(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` は `visible_window_owners` が渡した生きている `Vec`、`hwnd` は列挙で渡されたウィンドウ。
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (*(lparam.0 as *mut Vec<u32>)).push(pid);
        }
    }
    TRUE
}

/// 見えているトップレベルのウィンドウを持つプロセスの ID（同じ ID が並ぶこともある）。
fn visible_window_owners() -> Option<Vec<u32>> {
    let mut owners: Vec<u32> = Vec::new();
    let lparam = LPARAM(&mut owners as *mut Vec<u32> as isize);
    // SAFETY: コールバックは列挙の間だけ `owners` を使い、列挙は戻る前に終わる。
    unsafe { EnumWindows(Some(push_visible_owner), lparam) }.ok()?;
    Some(owners)
}

/// 同じデスクトップに見えるウィンドウを持つ、自分以外の movie-maker.exe の数。順番待ちの再生テストはまだウィンドウを
/// 作っていないので数えない。数えられなければ `None`。
pub(super) fn other_movie_maker_windows() -> Option<usize> {
    let owners = visible_window_owners()?;
    let me = std::process::id();
    let count = processes()?
        .iter()
        .filter(|(pid, exe)| {
            *pid != me && exe.eq_ignore_ascii_case(EXE_NAME) && owners.contains(pid)
        })
        .count();
    Some(count)
}

/// 前面のウィンドウを持つプロセス。自分なら `"self"`、ほかなら実行ファイルの名前と ID、前面のウィンドウが無ければ null。
pub(super) fn foreground_owner() -> Value {
    // SAFETY: 引数の無い問い合わせ。
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Value::Null;
    }
    let mut pid = 0;
    // SAFETY: `hwnd` は前面のウィンドウ。問い合わせの間に閉じられても ID が 0 になるだけ。
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == std::process::id() {
        return json!("self");
    }
    let exe = processes()
        .and_then(|list| list.into_iter().find(|(p, _)| *p == pid))
        .map(|(_, exe)| exe);
    json!(format!("{} (pid {pid})", exe.as_deref().unwrap_or("?")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 動いているプロセスに自分が入っている() {
        let me = std::process::id();
        assert!(processes().unwrap().iter().any(|(pid, _)| *pid == me));
    }

    #[test]
    fn ほかの_movie_maker_のウィンドウを数えられる() {
        assert!(other_movie_maker_windows().is_some());
    }

    #[test]
    fn 前面のウィンドウを持つプロセスは名前か_null() {
        let owner = foreground_owner();
        assert!(owner.is_null() || owner.is_string(), "{owner}");
    }
}
