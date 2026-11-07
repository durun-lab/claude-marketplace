//! ふりがなの場所。ふりがなはその行の親文字の上に描くので、ルビを含む行の上にふりがなの高さの分を空ける。
//! 1 行目は `bounds` を上に広げ（字は動かさない）、2 行目以降はその行から下を下げて前の行に重ならないようにする。

use std::ops::Range;

use mm_text_api::{parse_ruby, ShapedLine, ShapedText, TextStyle, RUBY_SCALE};

/// ふりがなの高さ（1 倍の画素）。描く実装はこの高さの中に描く。
pub(crate) fn ruby_height(style: &TextStyle) -> f32 {
    style.size_px * RUBY_SCALE
}

/// 行のグリフが指す元の文字列の範囲の中に、親文字の始まりがあるか。
fn has_ruby(line: &ShapedLine, bases: &[Range<usize>]) -> bool {
    let lo = line.glyphs.iter().map(|g| g.byte_offset).min();
    let hi = line.glyphs.iter().map(|g| g.byte_offset + g.byte_len).max();
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return false;
    };
    bases.iter().any(|b| (lo..hi).contains(&b.start))
}

fn move_down(line: &mut ShapedLine, dy: f32) {
    line.rect.y += dy;
    for g in &mut line.glyphs {
        g.rect.y += dy;
        g.origin[1] += dy;
    }
}

/// 2 行目以降のルビを含む行から下を下げ、`bounds` の高さに足す。1 行目がルビを含むかを返す
/// （含めば呼ぶ側が `bounds` を上に広げる）。`shaped.text` は組んだ元の文字列であること。
pub(crate) fn make_room(shaped: &mut ShapedText, style: &TextStyle) -> bool {
    let bases: Vec<Range<usize>> = parse_ruby(&shaped.text)
        .into_iter()
        .filter(|s| s.ruby.is_some())
        .map(|s| s.base)
        .collect();
    if bases.is_empty() {
        return false;
    }
    let height = ruby_height(style);
    let mut shift = 0.0;
    let mut first = false;
    for (i, line) in shaped.lines.iter_mut().enumerate() {
        match (has_ruby(line, &bases), i) {
            (true, 0) => first = true,
            (true, _) => shift += height,
            (false, _) => {}
        }
        move_down(line, shift);
    }
    shaped.bounds.height += shift;
    first
}
