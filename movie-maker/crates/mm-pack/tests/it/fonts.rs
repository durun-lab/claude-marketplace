//! パックの書体の目録（`fonts/fonts.json`）。読み込み、検査、キャッシュへの取得と照合、プロジェクトへの写しと素材の操作。
//! 取得は差し替えの口（`Fetch`）で、ネットワークを使わずに確かめる。

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mm_doc::Project;
use mm_ops::{Editor, OpContext, Transaction};
use mm_pack::{check, copy_fonts, font_asset_ops, font_files, Fetch, FontCache, Pack};
use mm_types::{MmError, Result};

use crate::vlog::vlog;

const FAMILIES: [&str; 3] = ["Dela Gothic One", "M PLUS Rounded 1c", "Mochiy Pop One"];
const FAMILY: &str = "Mochiy Pop One";
const FILE: &str = "MochiyPopOne-Regular.ttf";
const URL: &str = "https://example.invalid/fonts/MochiyPopOne-Regular.ttf";
/// 配布元を指す URL の頭。コミットの 40 桁で固定する。
const PINNED: &str = "https://raw.githubusercontent.com/google/fonts/";
const COMMIT_LEN: usize = 40;

/// テスト用に数文字を切り出した書体（fixtures/fonts）。
fn fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(FAMILY)
        .join(FILE);
    std::fs::read(path).unwrap()
}

fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// URL → 中身の表で答える取得の口。呼ばれた回数を数える。表に無い URL はオフラインと同じ誤り。
struct Table {
    files: BTreeMap<String, Vec<u8>>,
    calls: Cell<usize>,
}

impl Table {
    fn new(files: &[(&str, Vec<u8>)]) -> Self {
        let files = files
            .iter()
            .map(|(u, b)| (u.to_string(), b.clone()))
            .collect();
        Self {
            files,
            calls: Cell::new(0),
        }
    }
}

impl Fetch for Table {
    fn fetch(&self, url: &str) -> Result<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| MmError::NotFound(format!("{url}: オフライン")))
    }
}

/// `pack.json` と `theme.json` だけのパックに、目録と `fonts/<family>/OFL.txt` を作る。
fn small_pack(
    manifest: Option<serde_json::Value>,
    licensed: &[&str],
) -> (tempfile::TempDir, PathBuf) {
    let vlog_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/vlog");
    let dir = tempfile::tempdir().unwrap();
    for file in ["pack.json", "theme.json"] {
        std::fs::copy(vlog_dir.join(file), dir.path().join(file)).unwrap();
    }
    let fonts = dir.path().join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    if let Some(m) = manifest {
        std::fs::write(fonts.join("fonts.json"), m.to_string()).unwrap();
    }
    for family in licensed {
        std::fs::create_dir_all(fonts.join(family)).unwrap();
        std::fs::write(fonts.join(family).join("OFL.txt"), b"OFL").unwrap();
    }
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn entry(family: &str, file: &str, url: &str, blake3: &str) -> serde_json::Value {
    serde_json::json!({ "family": family, "file": file, "url": url, "blake3": blake3 })
}

/// fixture の書体 1 つを目録に持つパック。
fn fixture_pack() -> (tempfile::TempDir, Pack) {
    let manifest = serde_json::json!({ "fonts": [entry(FAMILY, FILE, URL, &hash(&fixture()))] });
    let (keep, dir) = small_pack(Some(manifest), &[FAMILY]);
    (keep, Pack::load(&dir).unwrap())
}

#[test]
fn vlog_の目録はテーマの_3_書体を固定のコミットの_url_とハッシュで持つ() {
    let pack = vlog();
    let mut families: Vec<&str> = pack.fonts.iter().map(|f| f.family.as_str()).collect();
    families.dedup();
    assert_eq!(families, FAMILIES);
    for family in pack.theme.fonts.values() {
        assert!(FAMILIES.contains(&family.as_str()), "{family}");
    }
    for font in &pack.fonts {
        let rest = font.url.strip_prefix(PINNED).expect(&font.url);
        let commit = rest.split('/').next().unwrap();
        assert_eq!(commit.len(), COMMIT_LEN, "{}", font.url);
        assert!(
            font.url.ends_with(&format!("/{}", font.file)),
            "{}",
            font.url
        );
        assert_eq!(font.blake3.len(), 64);
    }
    assert!(check(&pack).errors.is_empty(), "{:?}", check(&pack).errors);
}

#[test]
fn 取った書体はハッシュを照らしてキャッシュに置き_2_回目は取らない() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    let table = Table::new(&[(URL, fixture())]);
    assert!(cache.resolve(&pack.fonts).is_empty());
    let report = cache.fetch(&pack.fonts, &table);
    assert_eq!((report.fetched, report.cached), (1, 0));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let have = cache.resolve(&pack.fonts);
    assert_eq!(have.len(), 1);
    assert!(have[0].path.starts_with(cache_dir.path()));
    assert_eq!(std::fs::read(&have[0].path).unwrap(), fixture());
    let again = cache.fetch(&pack.fonts, &table);
    assert_eq!((again.fetched, again.cached), (0, 1));
    assert_eq!(table.calls.get(), 1);
}

#[test]
fn ハッシュの違うファイルは置かずに使わない() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    let report = cache.fetch(&pack.fonts, &Table::new(&[(URL, b"not a font".to_vec())]));
    assert_eq!(report.fetched, 0);
    assert!(
        report.failed[0].1.contains("ハッシュ"),
        "{:?}",
        report.failed
    );
    assert!(cache.resolve(&pack.fonts).is_empty());
    assert!(!cache.path(&pack.fonts[0]).exists());
}

#[test]
fn 取れないときは落ちずに空の組になり_os_の書体で描く() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    let report = cache.fetch(&pack.fonts, &Table::new(&[]));
    assert_eq!(report.failed.len(), 1);
    assert!(report.failed[0].1.contains("オフライン"));
    let have = cache.resolve(&pack.fonts);
    assert!(font_files(&have).is_empty());
    assert!(font_asset_ops(&have, &Project::new()).is_empty());
}

#[test]
fn 配布元に届かなければ残りの書体は取りにいかない() {
    let h = hash(&fixture());
    let manifest = serde_json::json!({ "fonts": [
        entry(FAMILY, FILE, URL, &h),
        entry(FAMILY, "Other.ttf", "https://example.invalid/Other.ttf", &h),
    ] });
    let (_keep, dir) = small_pack(Some(manifest), &[FAMILY]);
    let pack = Pack::load(&dir).unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let table = Table::new(&[]);
    let report = FontCache::new(cache_dir.path()).fetch(&pack.fonts, &table);
    assert_eq!(report.failed.len(), 2);
    assert_eq!(table.calls.get(), 1);
}

#[test]
fn キャッシュの中身が壊れていたら使わず_取り直す() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    let path = cache.path(&pack.fonts[0]);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"broken").unwrap();
    assert!(cache.resolve(&pack.fonts).is_empty());
    let report = cache.fetch(&pack.fonts, &Table::new(&[(URL, fixture())]));
    assert_eq!(report.fetched, 1);
    assert_eq!(cache.resolve(&pack.fonts).len(), 1);
}

#[test]
fn 書体の素材はプロジェクトのフォルダーからの場所とハッシュで足し_2_回目は足さない() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    cache.fetch(&pack.fonts, &Table::new(&[(URL, fixture())]));
    let have = cache.resolve(&pack.fonts);
    let files = font_files(&have);
    assert_eq!(files[0].family.as_deref(), Some(FAMILY));
    let ops = font_asset_ops(&have, &Project::new());
    assert_eq!(ops.len(), 1);
    let mut editor = Editor::new(Project::new());
    editor
        .apply(Transaction::new("書体", ops), &OpContext::human())
        .expect("素材を足せる");
    let project = editor.project().clone();
    let asset = &project.assets[0];
    assert_eq!(asset.id.as_str(), "font-mochiypopone-regular");
    assert_eq!(asset.path, format!("fonts/{FAMILY}/{FILE}"));
    assert_eq!(asset.content_hash, hash(&fixture()));
    assert!(font_asset_ops(&have, &project).is_empty());
}

#[test]
fn キャッシュの書体とパックのライセンスの文をプロジェクトに写し_同じものは写し直さない() {
    let (_keep, pack) = fixture_pack();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = FontCache::new(cache_dir.path());
    cache.fetch(&pack.fonts, &Table::new(&[(URL, fixture())]));
    let have = cache.resolve(&pack.fonts);
    let project = tempfile::tempdir().unwrap();
    assert_eq!(copy_fonts(&have, &pack.dir, project.path()).unwrap(), 2);
    let to = project.path().join("fonts").join(FAMILY);
    assert_eq!(std::fs::read(to.join(FILE)).unwrap(), fixture());
    assert!(to.join("OFL.txt").is_file());
    assert_eq!(copy_fonts(&have, &pack.dir, project.path()).unwrap(), 0);
}

#[test]
fn 検査はテーマの書体の抜けとライセンスの文の抜けと_https_でない_url_を捕まえる() {
    let h = hash(b"x");
    let manifest = serde_json::json!({ "fonts": [
        entry("Dela Gothic One", "a.ttf", "https://example.invalid/a.ttf", &h),
        entry("M PLUS Rounded 1c", "b.ttf", "http://example.invalid/b.ttf", &h),
    ] });
    let (_keep, dir) = small_pack(Some(manifest), &["Dela Gothic One"]);
    let errors = check(&Pack::load(&dir).unwrap()).errors;
    let has = |a: &str, b: &str| errors.iter().any(|e| e.contains(a) && e.contains(b));
    assert!(has("Mochiy Pop One", "fonts.json"), "{errors:?}");
    assert!(has("M PLUS Rounded 1c", "ライセンス"), "{errors:?}");
    assert!(has("b.ttf", "HTTPS"), "{errors:?}");
    assert!(
        !errors.iter().any(|e| e.contains("Dela Gothic One")),
        "{errors:?}"
    );
}

#[test]
fn 目録の名前に区切りや短いハッシュがあれば読めない() {
    let good = hash(b"x");
    for bad in [
        entry("../x", "a.ttf", URL, &good),
        entry(FAMILY, "a/b.ttf", URL, &good),
        entry(FAMILY, "a.ttf", URL, "../.."),
    ] {
        let (_keep, dir) = small_pack(Some(serde_json::json!({ "fonts": [bad] })), &[]);
        assert!(Pack::load(&dir).is_err());
    }
}

#[test]
fn 目録の無いパックは書体を検査しない() {
    let (_keep, dir) = small_pack(None, &[]);
    let pack = Pack::load(&dir).unwrap();
    assert!(pack.fonts.is_empty());
    assert!(!check(&pack).errors.iter().any(|e| e.contains("書体")));
}
