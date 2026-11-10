//! ホストの能力の問い合わせと、起動に失敗したときの片付け（CEF の要らないテストを含む）。

use std::path::Path;

use mm_host_cef::{supports_cef, CEF_PATH_ENV};

/// このテストと同じ feature で作ったホストの実行ファイル。
fn built_host() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_mm-host-cef"))
}

#[test]
fn cef_の_feature_つきのホストだけを使う() {
    // feature 無しのホストは何も答えずに終わり、`libcef.dll` が見つからないホストは起動できない。
    let expected = cfg!(feature = "cef") && std::env::var_os(CEF_PATH_ENV).is_some();
    assert_eq!(supports_cef(built_host()), expected);
}

#[test]
fn 起動できない実行ファイルは使わない() {
    assert!(!supports_cef(Path::new("mm-host-cef-が無い場所/none.exe")));
}

/// このプロセスの pid を名前に持つホストの一時フォルダー（`mm-<ランチャーの KIND.name>-<pid>-<番号>`）。
/// 前に同じ pid だったプロセスが残したものも入るので、起こす前と比べて使う。
pub(crate) fn work_dirs() -> Vec<std::path::PathBuf> {
    let prefix = format!("mm-host_cef-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(feature = "cef"))]
#[test]
fn 起動に失敗したホストの一時フォルダーを残さない() {
    use mm_host_cef::CefLauncher;
    let spec = mm_host_api::HostSpec {
        executable: built_host().to_path_buf(),
        args: Vec::new(),
        default_timeout: std::time::Duration::from_secs(1),
        gpu: None,
        client_pid: None,
    };
    let before = work_dirs();
    assert!(
        CefLauncher::start(spec).is_err(),
        "feature 無しのホストは起動しない"
    );
    let left: Vec<_> = work_dirs()
        .into_iter()
        .filter(|d| !before.contains(d))
        .collect();
    assert_eq!(left, Vec::<std::path::PathBuf>::new());
}
