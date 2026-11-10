//! 再生位置のタイムコードの 1 字ずつのシェーピング（[`TextMeasure::timecode_stops`]）。

use super::{installed, shaper};
use crate::fonts::Shaper;
use mm_ui_layout::TextMeasure;

#[test]
fn 再生位置のタイムコードは1字ずつキャッシュし_数字が変わってもシェーピングし直さない() {
    let mut s = shaper();
    let a = s.timecode_stops("00:00:00:00", 12.0);
    assert_eq!(s.cache().count(), 11, "0〜9 と「:」");
    let b = s.timecode_stops("11:59:23:17", 12.0);
    assert_eq!(
        s.cache().count(),
        11,
        "数字が変わってもキャッシュした字を並べる"
    );
    assert_eq!(a, b, "等幅の数字なので、どの数字でも同じ位置に並ぶ");
    let wide = ('0'..='9')
        .filter_map(|d| s.cache().timecode_advance(d, 12.0))
        .fold(0.0_f32, f32::max);
    assert_eq!(s.cache().timecode_advance('1', 12.0), Some(wide));
}

#[test]
fn 数字が等幅のフォントなら_1字ずつ並べた位置は文字列ごとのシェーピングと同じ() {
    let mut s = shaper();
    if !installed(&s, "Tahoma") {
        return;
    }
    for size in [11.0, 12.0] {
        let each = s.timecode_stops("12:34:56:78", size);
        let whole = s.stops("12:34:56:78", size).to_vec();
        assert_eq!(each.len(), whole.len());
        let gap = each.iter().zip(&whole).map(|(e, w)| (e - w).abs());
        assert!(
            gap.fold(0.0, f32::max) < 0.01,
            "{size}: {each:?} と {whole:?}"
        );
    }
}

#[test]
fn 字の大きさごとに1字ずつキャッシュし_フォントを替えたら捨ててシェーピングし直す() {
    let mut s = shaper();
    s.timecode_stops("00:00", 12.0);
    s.timecode_stops("00:00", 11.0);
    assert_eq!(s.cache().count(), 22, "字の大きさごとに 0〜9 と「:」");
    let advance = |s: &Shaper, size| {
        s.cache()
            .timecode_advance('0', size)
            .expect("キャッシュした")
    };
    assert!(advance(&s, 11.0) < advance(&s, 12.0));
    s.set_family("Segoe UI");
    assert_eq!(s.cache().timecode_advance('0', 12.0), None);
    s.timecode_stops("00:00", 12.0);
    assert_eq!(s.cache().count(), 33, "替えたフォントでシェーピングし直す");
}

#[test]
fn テーマのフォントに無い字は_1字ずつでもフォールバックのフォントでシェーピングする() {
    let mut s = shaper();
    if !installed(&s, "Segoe UI Symbol") {
        return;
    }
    let stops = s.timecode_stops("★1", 12.0);
    let star = s.cache().get("★", 12.0).expect("1 字ずつキャッシュする");
    let run = star.buffer.layout_runs().next().expect("1 行");
    assert!(run.glyphs.iter().all(|g| g.glyph_id != 0), "豆腐にしない");
    assert_eq!(stops[0], star.advance());
}
