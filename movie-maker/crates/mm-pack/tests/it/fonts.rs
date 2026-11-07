//! パックの書体（`fonts/<family>/`）。読み込み、検査、プロジェクトへの写しと素材の操作。

use std::path::{Path, PathBuf};

use mm_doc::Project;
use mm_ops::{Editor, OpContext, Transaction};
use mm_pack::{check, copy_fonts, font_asset_ops, font_files, Pack};

use crate::vlog::vlog;

const FAMILIES: [&str; 3] = ["Dela Gothic One", "M PLUS Rounded 1c", "Mochiy Pop One"];

#[test]
fn vlog_のパックはテーマの_3_書体を_family_のフォルダーに持つ() {
    let pack = vlog();
    let mut families: Vec<&str> = pack.fonts.iter().map(|f| f.family.as_str()).collect();
    families.dedup();
    assert_eq!(families, FAMILIES);
    for family in pack.theme.fonts.values() {
        assert!(FAMILIES.contains(&family.as_str()), "{family}");
    }
    let files = font_files(&pack.fonts);
    assert!(files.iter().all(|f| f.family.is_some() && f.path.is_file()));
}

#[test]
fn 書体の素材はプロジェクトのフォルダーからの場所とハッシュで足し_2_回目は足さない() {
    let pack = vlog();
    let ops = font_asset_ops(&pack.fonts, &Project::new()).unwrap();
    assert_eq!(ops.len(), pack.fonts.len());
    let mut editor = Editor::new(Project::new());
    editor
        .apply(Transaction::new("書体", ops), &OpContext::human())
        .expect("素材を足せる");
    let project = editor.project().clone();
    let asset = project
        .assets
        .iter()
        .find(|a| a.id.as_str() == "font-mochiypopone-regular")
        .expect("Mochiy Pop One の素材");
    assert_eq!(asset.path, "fonts/Mochiy Pop One/MochiyPopOne-Regular.ttf");
    assert_eq!(asset.content_hash.len(), 64);
    assert!(font_asset_ops(&pack.fonts, &project).unwrap().is_empty());
}

#[test]
fn 書体とライセンスの文をプロジェクトの_fonts_に写し_同じものは写し直さない() {
    let pack = vlog();
    let dir = tempfile::tempdir().unwrap();
    let copied = copy_fonts(&pack.fonts, dir.path()).unwrap();
    assert_eq!(copied, pack.fonts.len() + FAMILIES.len(), "書体と OFL.txt");
    for family in FAMILIES {
        assert!(dir
            .path()
            .join("fonts")
            .join(family)
            .join("OFL.txt")
            .is_file());
    }
    assert_eq!(copy_fonts(&pack.fonts, dir.path()).unwrap(), 0);
}

/// `pack.json` と `theme.json` だけのパックに、`fonts/<family>/<file>` を作る。
fn small_pack(fonts: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let vlog_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/vlog");
    let dir = tempfile::tempdir().unwrap();
    for file in ["pack.json", "theme.json"] {
        std::fs::copy(vlog_dir.join(file), dir.path().join(file)).unwrap();
    }
    for (family, file) in fonts {
        let at = dir.path().join("fonts").join(family);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join(file), b"font").unwrap();
    }
    let path = dir.path().to_path_buf();
    (dir, path)
}

#[test]
fn 検査はテーマの書体のフォルダーとライセンスの文が無いのを捕まえる() {
    let (_keep, dir) = small_pack(&[
        ("Dela Gothic One", "a.ttf"),
        ("Dela Gothic One", "OFL.txt"),
        ("M PLUS Rounded 1c", "b.ttf"),
    ]);
    let errors = check(&Pack::load(&dir).unwrap()).errors;
    assert!(
        errors.iter().any(|e| e.contains("fonts/Mochiy Pop One/")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("M PLUS Rounded 1c") && e.contains("ライセンス")),
        "{errors:?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("Dela Gothic One")),
        "{errors:?}"
    );
}

#[test]
fn 書体を持たないパックは書体を検査しない() {
    let (_keep, dir) = small_pack(&[]);
    let pack = Pack::load(&dir).unwrap();
    assert!(pack.fonts.is_empty());
    assert!(!check(&pack).errors.iter().any(|e| e.contains("書体")));
}
