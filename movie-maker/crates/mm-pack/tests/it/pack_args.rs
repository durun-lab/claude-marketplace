//! マクロの引数の検査（真偽、列挙、区切りのある書式）と、出る・消える動きの長さ、ページの色の直書き。

use mm_pack::Pack;
use mm_types::MmError;

use crate::vlog::{args, check_broken, expand_est, mentions, vlog};

fn rejected(name: &str, pairs: &[(&str, &str)]) -> String {
    let mut all = vec![("id", "x"), ("at", "0")];
    all.extend_from_slice(pairs);
    match expand_est(&vlog(), name, &args(&all)) {
        Err(MmError::InvalidArgument(m)) => m,
        other => panic!("{name} {pairs:?} は拒む: {other:?}"),
    }
}

#[test]
fn 真偽と列挙の引数は展開の前に拒む() {
    assert!(rejected("marks", &[("dash", "yes")]).contains("dash"));
    assert!(rejected("marks", &[("kind", "circle")]).contains("kind"));
    assert!(rejected("marks", &[("tail", "left")]).contains("tail"));
    assert!(
        rejected("figure", &[("head", "h"), ("labels", "a"), ("side", "top")]).contains("side")
    );
    assert!(rejected(
        "figure",
        &[("head", "h"), ("labels", "a"), ("kinds", "dry wet")]
    )
    .contains("kinds"));
    assert!(rejected("card", &[("plain", "True")]).contains("plain"));
    assert!(rejected("tip", &[("head", "h"), ("body", "b"), ("tail", "side")]).contains("tail"));
    assert!(rejected("cta", &[("icon", "heart")]).contains("icon"));
}

#[test]
fn 区切りのある引数の壊れた値は展開の前に拒む() {
    assert!(rejected("marks", &[("pts", "1,2,3")]).contains("pts"));
    assert!(rejected("marks", &[("box", "1,2,3")]).contains("box"));
    let fig = [("head", "h"), ("labels", "a|b")];
    let with = |k: &'static str, v: &'static str| {
        let mut p = fig.to_vec();
        p.push((k, v));
        rejected("figure", &p)
    };
    assert!(with("cues", "2@x+1").contains("cues"));
    assert!(with("row_at", "0 abc").contains("row_at"));
    assert!(rejected("lyrics", &[("words", "ことば@1")]).contains("words"));
    assert!(rejected("lyrics", &[("words", "a@0+1"), ("notes", "do@0+1")]).contains("notes"));
    assert!(rejected("lyrics", &[("words", "a@0+1"), ("bars", "0 x")]).contains("bars"));
    let read = [("tab", "t"), ("doc", "d")];
    let mut p = read.to_vec();
    p.push(("segs", "a@0+1|b"));
    assert!(rejected("read", &p).contains("segs"));
}

#[test]
fn 正しい値は通る() {
    let pack = vlog();
    let ok = args(&[
        ("id", "m"),
        ("at", "0"),
        ("kind", "arrow"),
        ("pts", "1,2 3.5,4"),
        ("dash", "false"),
        ("tail", "dn"),
    ]);
    assert!(expand_est(&pack, "marks", &ok).is_ok());
}

#[test]
fn 動きの長さの和が出ている長さを超えると超えた秒数を示して拒む() {
    let m = rejected("marks", &[("dur", "0.3")]);
    assert!(m.contains("0.1"), "超えた秒数を出す: {m}");
    assert!(expand_est(
        &vlog(),
        "marks",
        &args(&[("id", "x"), ("at", "0"), ("dur", "0.4")])
    )
    .is_ok());
}

#[test]
fn 検査は壊した引数の定義を捕まえる() {
    let errors = check_broken("macros/marks.json", |v| {
        v["params"]["kind"]["default"] = "circle".into();
        v["params"]["dash"]["default"] = "maybe".into();
        v["params"]["at"]["choices"] = serde_json::json!(["1"]);
    });
    assert!(mentions(&errors, "引数 kind: 既定の値が誤り"), "{errors:?}");
    assert!(mentions(&errors, "引数 dash: 既定の値が誤り"), "{errors:?}");
    assert!(
        mentions(&errors, "引数 at: choices と format"),
        "{errors:?}"
    );
}

#[test]
fn 検査は縁の固定の誤りを捕まえる() {
    let errors = check_broken("macros/claude.json", |v| {
        v["params"]["x"]["pin"]["anchor"] = 2.into();
        v["params"]["text"]["pin"] = serde_json::json!({"axis": "y", "anchor": 0});
    });
    assert!(
        mentions(&errors, "引数 x: pin の anchor は 0..1"),
        "{errors:?}"
    );
    assert!(
        mentions(&errors, "引数 text: pin は type が number"),
        "{errors:?}"
    );
}

#[test]
fn 検査はページの色の直書きを捕まえる() {
    let dir = tempfile::tempdir().unwrap();
    crate::vlog::copy_dir(&crate::vlog::vlog_dir(), dir.path());
    let path = dir.path().join("web/tip.html");
    let html = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        html.replace("</style>", "p { color: #2459E0; }\n</style>"),
    )
    .unwrap();
    let errors = mm_pack::check(&Pack::load(dir.path()).unwrap()).errors;
    assert!(mentions(&errors, "web/tip.html: 色 #2459E0"), "{errors:?}");
}
