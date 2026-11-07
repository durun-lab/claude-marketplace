//! Tauri は `frontendDist`（`ui/dist/`）が無いとビルドできない。`ui/dist/` は `pnpm build` の生成物で git に入れないので、
//! 無いときだけ画面の無い仮の index.html を置く（CI のように UI を作らない設定でも Rust が通る）。

use std::path::{Path, PathBuf};

const DIST_INDEX: &str = "../../ui/dist/index.html";
const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>movie-maker</title>\
<body>ui/dist/ がありません。movie-maker/ui で pnpm install && pnpm build を実行してください。";

/// tauri-build（tauri-winres と embed-resource）が書く Windows のリソース。中身は `.res` で、リンカーに渡る。
const TAURI_RESOURCE: &str = "resource.lib";
/// Tauri の既定のマニフェスト（comctl32 v6 の依存）。無いと起動時に 0xc0000139 で落ちる。
const APP_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#;
/// `.res` の項目の型と名前（`RT_MANIFEST` と、実行ファイルのマニフェストの ID 1）。
const RT_MANIFEST: u16 = 24;
const MANIFEST_ID: u16 = 1;
/// `.res` の項目の頭の大きさ（型と名前を番号で持つとき）。
const RES_HEADER_SIZE: u32 = 32;
/// 項目のメモリーの印（MOVEABLE と PURE。rc.exe の既定）と言語（英語、米国）。
const RES_MEMORY_FLAGS: u16 = 0x0030;
const RES_LANGUAGE: u16 = 0x0409;

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
        keep_only_manifest();
    }
}

/// `dx serve --hot-patch` は自前の Windows のリソース（版の情報とアイコン）をリンクするので、tauri-build のリソースと
/// 二重になり `duplicate resource` でリンクが失敗する。開発のときだけ tauri-build の側をマニフェストだけの `.res` に置き換える。
/// マニフェストをリンカーの引数（/MANIFESTDEPENDENCY）で渡さないのは、dx がリンカーの引数を空白で切るため。
fn keep_only_manifest() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    let Some(out) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    let resource = out.join(TAURI_RESOURCE);
    if !msvc || !resource.is_file() {
        return;
    }
    if let Err(e) = std::fs::write(&resource, manifest_res(APP_MANIFEST.as_bytes())) {
        panic!("{} を置き換えられない: {e}", resource.display());
    }
}

/// マニフェスト 1 つだけを持つ `.res` を作る。先頭は中身の無い項目（`.res` の決まり）。
fn manifest_res(manifest: &[u8]) -> Vec<u8> {
    let mut res = Vec::new();
    push_entry(&mut res, 0, 0, &[], 0, 0);
    push_entry(
        &mut res,
        RT_MANIFEST,
        MANIFEST_ID,
        manifest,
        RES_MEMORY_FLAGS,
        RES_LANGUAGE,
    );
    res
}

/// `.res` の項目を 1 つ足す。型と名前は番号で持ち、中身は 4 バイトの境目まで 0 で埋める。
fn push_entry(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8], flags: u16, lang: u16) {
    let size = u32::try_from(data.len()).expect("マニフェストが大きすぎる");
    res.extend_from_slice(&size.to_le_bytes());
    res.extend_from_slice(&RES_HEADER_SIZE.to_le_bytes());
    for word in [0xFFFF, kind, 0xFFFF, name] {
        res.extend_from_slice(&u16::to_le_bytes(word));
    }
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&lang.to_le_bytes());
    res.extend_from_slice(&[0; 8]);
    res.extend_from_slice(data);
    res.resize(res.len().next_multiple_of(4), 0);
}
