//! ホストの側でプラグインの共有ライブラリを読み込む（`mm-host-ofx`、`mm-host-clap`、`mm-host-vst3` で共通）。

use std::path::Path;

/// 共有ライブラリを読み込む。Windows では依存の DLL をプラグインのフォルダーとシステムのフォルダーから探し、
/// カレントや `PATH` の同じ名前の DLL を読まない（DLL の差し込みを防ぐ）。
///
/// # Safety
///
/// 共有ライブラリの初期化（`DllMain`）が動く。呼ぶのはクラッシュしてもそのプロセスだけが止まる別プロセスのホストに限る。
#[cfg(windows)]
pub unsafe fn load_library(binary: &Path) -> Result<libloading::Library, String> {
    use libloading::os::windows::{
        Library, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };
    // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR は完全なパスでないと受けない。
    let binary = std::path::absolute(binary).map_err(|e| e.to_string())?;
    let flags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
    // SAFETY: 呼び出し側の約束（この関数の Safety）に従う。
    unsafe { Library::load_with_flags(&binary, flags) }
        .map(Into::into)
        .map_err(|e| format!("{} を読み込めない: {e}", binary.display()))
}

/// 共有ライブラリを読み込む。
///
/// # Safety
///
/// 共有ライブラリの初期化が動く。呼ぶのはクラッシュしてもそのプロセスだけが止まる別プロセスのホストに限る。
#[cfg(not(windows))]
pub unsafe fn load_library(binary: &Path) -> Result<libloading::Library, String> {
    // SAFETY: 呼び出し側の約束（この関数の Safety）に従う。
    unsafe { libloading::Library::new(binary) }
        .map_err(|e| format!("{} を読み込めない: {e}", binary.display()))
}
