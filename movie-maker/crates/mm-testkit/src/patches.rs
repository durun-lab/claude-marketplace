//! 純色のパッチ。8 bit の色（16 進の値）が各経路（文字、web.cef、PNG の素材、パックの theme）で線形になり、
//! 書き出しの符号化値（Rec.1886、`mm_types::transfer`）で元の値に戻るかを比べる。

use mm_types::transfer::linear_to_code8;
use mm_types::{Rgba, RgbaImage};

/// パックの theme の 5 色（vlog の theme.json の名前と値）。`mm-pack` の試験が theme.json と同じかを確かめる。
pub const THEME_COLORS: [(&str, [u8; 3]); 5] = [
    ("sapphire", [0x24, 0x59, 0xe0]),
    ("yellow", [0xff, 0xd4, 0x3b]),
    ("ink", [0x0b, 0x12, 0x30]),
    ("paper", [0xf5, 0xf8, 0xff]),
    ("miku", [0x39, 0xc5, 0xbb]),
];

/// 灰（0、6、16、64、128、200、255）、ep04 の背景（#06090d）、theme の 5 色。
pub const PATCHES: [[u8; 3]; 13] = [
    [0, 0, 0],
    [6, 6, 6],
    [16, 16, 16],
    [64, 64, 64],
    [128, 128, 128],
    [200, 200, 200],
    [255, 255, 255],
    [0x06, 0x09, 0x0d],
    THEME_COLORS[0].1,
    THEME_COLORS[1].1,
    THEME_COLORS[2].1,
    THEME_COLORS[3].1,
    THEME_COLORS[4].1,
];

/// 符号化値で許す差。
pub const TOLERANCE: i32 = 1;
/// 被覆率が 1 とみなす alpha の下限。
const SOLID_ALPHA: f32 = 0.999;

/// 不透明のパッチの色を線形の premultiplied にする（16 進の色を読む経路と同じ式）。
pub fn color(rgb: [u8; 3]) -> Rgba {
    let [r, g, b] = rgb;
    Rgba::from_code8([r, g, b, u8::MAX])
}

/// 線形の premultiplied の画素を、alpha で割ってから書き出しと同じ式で 8 bit の符号化値にする。
pub fn encode(p: Rgba) -> [i32; 3] {
    let a = if p.a > 0.0 { p.a } else { 1.0 };
    [p.r, p.g, p.b].map(|c| i32::from(linear_to_code8(c / a)))
}

/// 画像の中で alpha の最も大きい画素（文字で描いた「■」の塗りつぶしの中）。被覆率が 1 に届かなければ失敗にする。
pub fn solid_pixel(img: &RgbaImage) -> Rgba {
    let mut best = Rgba::TRANSPARENT;
    for y in 0..img.height() {
        for x in 0..img.width() {
            let p = img.pixel(x, y);
            if p.a > best.a {
                best = p;
            }
        }
    }
    assert!(best.a >= SOLID_ALPHA, "塗りつぶしの画素が無い: {best:?}");
    best
}

/// 期待する符号化値（パッチの値そのまま）。
pub fn expected() -> Vec<[i32; 3]> {
    PATCHES.iter().map(|c| c.map(i32::from)).collect()
}

/// 全パッチの差が [`TOLERANCE`] に収まるかを確かめ、差の最大を返す。差の最大と値は標準エラーに出す。
pub fn assert_close(what: &str, got: &[[i32; 3]], want: &[[i32; 3]]) -> i32 {
    assert_eq!(got.len(), want.len(), "{what}: パッチの数");
    let worst = got
        .iter()
        .zip(want)
        .flat_map(|(g, w)| (0..3).map(move |c| (g[c] - w[c]).abs()))
        .max()
        .unwrap_or(0);
    eprintln!("{what}: 差の最大 {worst}、{got:?}");
    assert!(
        worst <= TOLERANCE,
        "{what} の符号化値が {want:?} から {worst} ずれる: {got:?}"
    );
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn パッチの色は書き出しの式で元の値に戻る() {
        let got: Vec<[i32; 3]> = PATCHES.iter().map(|&c| encode(color(c))).collect();
        assert_eq!(assert_close("式", &got, &expected()), 0);
    }

    #[test]
    fn 半透明の画素は割ってから符号化値にする() {
        let p = color([6, 9, 13]).scaled(0.5);
        assert_eq!(encode(p), [6, 9, 13]);
    }
}
