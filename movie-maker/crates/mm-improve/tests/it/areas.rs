//! リポジトリの docs/areas.toml で、領域の検査と自動の改善の取り込みの判定を確かめる。

use mm_improve::{Areas, ImproveVerdict};

fn areas() -> Areas {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/areas.toml");
    Areas::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn files(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn area_check_lists_shared_and_outside_files() {
    let changed = files(&[
        "crates/mm-improve/src/lib.rs",
        "Cargo.toml",
        "crates/mm-render/src/lib.rs",
    ]);
    let r = areas().check_area("improve", &changed).unwrap();
    assert_eq!(r.shared, files(&["Cargo.toml"]));
    assert_eq!(r.outside, files(&["crates/mm-render/src/lib.rs"]));
}

#[test]
fn panel_placeholder_matches_any_one_panel() {
    let r = areas()
        .check_area("ui-panels", &files(&["ui/panels/mixer/src/App.tsx"]))
        .unwrap();
    assert!(r.outside.is_empty(), "{r:?}");
}

#[test]
fn auto_writable_areas_are_merged() {
    let changed = files(&[
        "crates/mm-ops/src/ops/trim.rs",
        "ui/panels/review/src/a.ts",
        "packs/vlog/pack.toml",
    ]);
    assert_eq!(areas().improve_verdict(&changed), ImproveVerdict::Merge);
}

#[test]
fn approval_areas_are_held() {
    for f in [
        "crates/mm-render/src/lib.rs",
        "crates/mm-text-api/src/lib.rs",
        "crates/mm-doc/src/migrations/20261107_x.rs",
        "fixtures/golden/a.png",
        "crates/mm-improve/src/lib.rs",
        "apps/movie-maker/src/main.rs",
        "docs/areas.toml",
        "../durun-dashboard/server/improve.ts",
    ] {
        assert_eq!(
            areas().improve_verdict(&files(&[f])),
            ImproveVerdict::Hold(files(&[f])),
            "{f}"
        );
    }
}
