//! `.vst3` のバンドルの形（置き場所、中のモジュール）と、クラスの ID の文字の形。

use std::path::{Path, PathBuf};

use vst3::Steinberg::TUID;

/// バンドルの拡張子。
const EXTENSION: &str = "vst3";
/// 置き場所を走査する深さ。メーカーごとのフォルダーの 1 段下までを見る。
const SCAN_DEPTH: usize = 3;

/// このビルドの機械のバンドルの中のフォルダー（`Contents/<名前>`）。
fn arch_dirs() -> &'static [&'static str] {
    if cfg!(all(windows, target_arch = "x86_64")) {
        &["x86_64-win"]
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        &["arm64-win", "arm64x-win", "arm64ec-win"]
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        &["x86_64-linux"]
    } else {
        &[]
    }
}

/// 読み込むモジュールのファイル。フォルダーのバンドルなら `Contents/<機械>/<バンドルの名前>`、
/// ファイル（古い形の 1 つの DLL）ならそのもの。
pub fn module_file(path: &Path) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };
    let contents = path.join("Contents");
    let linux_name = Path::new(name).with_extension("so");
    arch_dirs()
        .iter()
        .flat_map(|arch| {
            let dir = contents.join(arch);
            [dir.join(name), dir.join(&linux_name)]
        })
        .find(|p| p.is_file())
        .unwrap_or_else(|| contents.join(arch_dirs().first().unwrap_or(&"")).join(name))
}

/// 利用者とメーカーが使う既定の置き場所（Windows は共通と利用者ごと）。
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            dirs.push(PathBuf::from(common).join("VST3"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Programs/Common/VST3"));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".vst3"));
        dirs.push(PathBuf::from("/usr/lib/vst3"));
    }
    dirs
}

/// `dir` の中の `.vst3`（バンドルのフォルダーか 1 つのファイル）を名前の順で返す。中のフォルダーも数段たどる。
pub fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    scan_into(dir, SCAN_DEPTH, &mut found);
    found.sort();
    found
}

fn scan_into(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let is_bundle = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
        if is_bundle {
            found.push(path);
        } else if depth > 1 && path.is_dir() {
            scan_into(&path, depth - 1, found);
        }
    }
}

/// クラスの ID を 32 桁の 16 進にする。プラグインのソースの `FUID (a, b, c, d)` と同じ並び
/// （Windows は COM の並びのバイト列から読み戻す）で、`moduleinfo.json` の `CID` と同じ文字になる。
pub fn class_id_hex(id: &TUID) -> String {
    let b = id.map(|x| x as u8);
    let words = if cfg!(windows) {
        [
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u32::from_le_bytes([b[6], b[7], b[4], b[5]]),
            u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
            u32::from_be_bytes([b[12], b[13], b[14], b[15]]),
        ]
    } else {
        [0, 4, 8, 12].map(|i| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]))
    };
    words.iter().map(|w| format!("{w:08X}")).collect()
}

/// [`class_id_hex`] の文字からクラスの ID に戻す。32 桁の 16 進でなければ `None`。
pub fn parse_class_id(text: &str) -> Option<TUID> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut words = [0u32; 4];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u32::from_str_radix(&text[i * 8..i * 8 + 8], 16).ok()?;
    }
    Some(vst3::uid(words[0], words[1], words[2], words[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn クラスの_id_は_fuid_の並びで往復する() {
        let id = vst3::uid(0x1234_5678, 0x9ABC_DEF0, 0x0FED_CBA9, 0x8765_4321);
        let text = class_id_hex(&id);
        assert_eq!(text, "123456789ABCDEF00FEDCBA987654321");
        assert_eq!(parse_class_id(&text), Some(id));
        assert_eq!(parse_class_id(&text.to_lowercase()), Some(id));
        assert_eq!(parse_class_id("12"), None);
        assert_eq!(parse_class_id(&"G".repeat(32)), None);
    }

    #[test]
    fn フォルダーのバンドルは中の機械のモジュールを指す() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bundle = dir.join("Foo.vst3");
        let inner = bundle.join("Contents").join(arch_dirs()[0]);
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("Foo.vst3"), b"").unwrap();
        std::fs::write(dir.join("Bar.vst3"), b"").unwrap();
        assert_eq!(module_file(&bundle), inner.join("Foo.vst3"));
        assert_eq!(module_file(&dir.join("Bar.vst3")), dir.join("Bar.vst3"));
        assert_eq!(scan_dir(dir), [dir.join("Bar.vst3"), bundle]);
    }
}
