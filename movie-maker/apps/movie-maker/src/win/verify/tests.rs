use super::*;

#[test]
fn スクラブの許容誤差は_1_物理ピクセルが長い全体表示で広がり_下限は_2_フレーム() {
    // 9:38 の全体表示（1 秒が約 1.25 物理ピクセル）。半画素は 0.4 秒。
    let wide = scrub_tolerance(1.25, 30.0);
    assert!((wide - (0.4 + 0.5 / 30.0)).abs() < 1e-6, "{wide}");
    // 1 秒 125 物理ピクセルなら 2 フレーム。ちょうど 2 フレームずれた結果も許す。
    let zoomed = scrub_tolerance(125.0, 30.0);
    assert!(
        (2.0 / 30.0..2.0 / 30.0 + 1e-6).contains(&zoomed),
        "{zoomed}"
    );
    assert!((3.066_666_666_666_667_f64 - 3.0).abs() <= zoomed);
}
