//! Tauri は `frontendDist`（`ui/dist/`）が無いとビルドできない。`ui/dist/` は `pnpm build` の生成物で git に入れないので、
//! 無いときだけ画面の無い仮の index.html を置く（CI のように UI を作らない設定でも Rust が通る）。

use std::path::{Path, PathBuf};

const DIST_INDEX: &str = "../../ui/dist/index.html";
const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>movie-maker</title>\
<body>ui/dist/ がありません。movie-maker/ui で pnpm install && pnpm build を実行してください。";

/// tauri-build（tauri-winres と embed-resource）が書く Windows のリソース。中身は `.res` で、リンカーに渡る。
const TAURI_RESOURCE: &str = "resource.lib";
/// リソースを 1 つも持たない `.res`（先頭の空の項目 32 バイトだけ）。
const EMPTY_RES: [u8; 32] = [
    0, 0, 0, 0, 0x20, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0,
];
/// Tauri の既定のマニフェストと同じ comctl32 v6 の依存。無いと起動時に 0xc0000139 で落ちる。
const COMCTL_DEPENDENCY: &str =
    "/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'";

fn main() {
    let index = Path::new(DIST_INDEX);
    if !index.exists() {
        if let Some(dir) = index.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(index, PLACEHOLDER);
    }
    println!("cargo:rerun-if-changed={DIST_INDEX}");
    tauri_build::build();
    if std::env::var_os("CARGO_FEATURE_HOTPATCH").is_some() {
        skip_tauri_resource();
    }
}

/// `dx serve --hotpatch` は自前の Windows のリソースをリンクするので、tauri-build のリソースと二重になり
/// `duplicate resource` でリンクが失敗する。開発のときだけ tauri-build の側を空にし、マニフェストはリンカーで埋める。
fn skip_tauri_resource() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    let Some(out) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    let resource = out.join(TAURI_RESOURCE);
    if !msvc || !resource.is_file() {
        return;
    }
    if let Err(e) = std::fs::write(&resource, EMPTY_RES) {
        panic!("{} を空にできない: {e}", resource.display());
    }
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins={COMCTL_DEPENDENCY}");
}
