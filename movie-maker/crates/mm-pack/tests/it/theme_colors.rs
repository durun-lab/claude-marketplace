//! パックの theme の色（16 進）が、線形にしてから書き出しの符号化値に戻すと元の値になるかの検査。

use mm_testkit::patches::{assert_close, encode, THEME_COLORS};

use crate::vlog::vlog;

#[test]
fn theme_の色は書き出しで同じ符号化値になる() {
    let pack = vlog();
    let got: Vec<[i32; 3]> = THEME_COLORS
        .iter()
        .map(|(name, _)| encode(pack.theme.color(name).expect("theme の色")))
        .collect();
    let want: Vec<[i32; 3]> = THEME_COLORS.iter().map(|(_, c)| c.map(i32::from)).collect();
    assert_close("パックの theme", &got, &want);
}

#[test]
fn パッチの_theme_の色は_theme_json_と同じ() {
    let pack = vlog();
    for (name, [r, g, b]) in THEME_COLORS {
        let hex = format!("#{r:02X}{g:02X}{b:02X}");
        assert_eq!(pack.theme.colors.get(name), Some(&hex), "{name}");
    }
}
