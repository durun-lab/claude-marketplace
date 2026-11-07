//! プロジェクトの書体（`use_fonts`）。テスト用に数文字を切り出した書体（fixtures/fonts）で、
//! 名前で引けることと、無いときに失敗しないことを確かめる。

use std::path::PathBuf;

use mm_text_api::{FontFile, ShapedText, TextRasterizer, TextShaper, TextStyle};
use mm_text_glyphon::TextEngine;

const FAMILY: &str = "Mochiy Pop One";
const POST_SCRIPT: &str = "MochiyPopOne-Regular";

fn pack_font(rel: &str) -> FontFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(rel);
    FontFile::from_asset(path, rel)
}

fn mochiy() -> FontFile {
    pack_font("Mochiy Pop One/MochiyPopOne-Regular.ttf")
}

fn style() -> TextStyle {
    TextStyle {
        font_family: FAMILY.into(),
        size_px: 48.0,
        ..TextStyle::default()
    }
}

fn faces(shaped: &ShapedText) -> Vec<&str> {
    shaped
        .faces
        .iter()
        .map(|f| f.post_script_name.as_str())
        .collect()
}

#[test]
fn プロジェクトの書体を名前で引ける() {
    let engine = TextEngine::new();
    let shaper = engine.shaper();
    shaper.use_fonts(&[mochiy()]).unwrap();
    let shaped = shaper.shape("おとABC", &style()).unwrap();
    assert_eq!(faces(&shaped), vec![POST_SCRIPT]);
}

#[test]
fn プロジェクトの書体で描いた画素は置き換えの書体と違う() {
    let text = "おとのはなし";
    let plain = TextEngine::new();
    let fallback = plain.shaper().shape(text, &style()).unwrap();
    let before = plain
        .rasterizer()
        .rasterize(&fallback, &style(), 1.0)
        .unwrap();
    let engine = TextEngine::new();
    engine.use_fonts(&[mochiy()]).unwrap();
    let shaped = engine.shaper().shape(text, &style()).unwrap();
    let after = engine
        .rasterizer()
        .rasterize(&shaped, &style(), 1.0)
        .unwrap();
    assert_ne!(
        faces(&fallback),
        faces(&shaped),
        "この PC に {FAMILY} が入っていると比べられない"
    );
    assert_ne!(before.data(), after.data());
}

#[test]
fn 書体のファイルが無くても失敗せずに置き換わる() {
    let engine = TextEngine::new();
    let baseline = engine.shaper().shape("おと", &style()).unwrap();
    let missing = pack_font("無い/None-Regular.ttf");
    engine.use_fonts(&[missing]).unwrap();
    let shaped = engine.shaper().shape("おと", &style()).unwrap();
    assert_eq!(shaped, baseline);
    let image = engine
        .rasterizer()
        .rasterize(&shaped, &style(), 1.0)
        .unwrap();
    assert!(image.data().iter().any(|v| *v > 0.0));
}

#[test]
fn 組を空にすると前の書体に戻る() {
    let engine = TextEngine::new();
    let shaper = engine.shaper();
    let baseline = shaper.shape("おと", &style()).unwrap();
    shaper.use_fonts(&[mochiy()]).unwrap();
    assert_eq!(
        faces(&shaper.shape("おと", &style()).unwrap()),
        vec![POST_SCRIPT]
    );
    shaper.use_fonts(&[]).unwrap();
    assert_eq!(shaper.shape("おと", &style()).unwrap(), baseline);
}

#[test]
fn フォルダーの名前で引けて_太さの違うファイルから既定の太さを選ぶ() {
    let engine = TextEngine::new();
    engine
        .use_fonts(&[
            pack_font("M PLUS Rounded 1c/MPLUSRounded1c-Regular.ttf"),
            pack_font("M PLUS Rounded 1c/MPLUSRounded1c-Black.ttf"),
        ])
        .unwrap();
    let style = TextStyle {
        font_family: "M PLUS Rounded 1c".into(),
        ..style()
    };
    let shaped = engine.shaper().shape("おと", &style).unwrap();
    assert_eq!(faces(&shaped), vec!["RoundedMplus1c-Regular"]);
}
