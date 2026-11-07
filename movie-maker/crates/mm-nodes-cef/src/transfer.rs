//! ページの色（8 bit の符号化値）を作業色空間の線形にする式。
//! 素材のデコーダーと書き出しは 8 bit の値を Rec.1886（べき 2.4）の符号化値として扱うので、ページの色も同じ式で線形にし、
//! CSS の `#06090d` が書き出しでも (6, 9, 13) に戻るようにする（sRGB の区分の式では暗部が持ち上がる）。

/// Rec.1886 の表示の式のべき（`mm-decode-ffmpeg` と `mm-encode-ffmpeg-core` の transfer と同じ）。
pub const GAMMA: f32 = 2.4;

/// sRGB の区分の式の折れ目（線形の側）。
const SRGB_KNEE: f32 = 0.003_130_8;
/// sRGB の区分の式の直線部の傾き。
const SRGB_SLOPE: f32 = 12.92;
/// sRGB の区分の式の曲線部の係数。
const SRGB_SCALE: f32 = 1.055;
const SRGB_OFFSET: f32 = 0.055;
/// sRGB の区分の式の曲線部のべき。値は Rec.1886 の [`GAMMA`] と同じ 2.4 だが、別の式の定数として分ける。
const SRGB_EXPONENT: f32 = 2.4;

/// 線形から sRGB の符号化値へ（IEC 61966-2-1）。ホストの CPU の経路が当てた式の逆。
fn linear_to_srgb(v: f32) -> f32 {
    if v <= SRGB_KNEE {
        v * SRGB_SLOPE
    } else {
        SRGB_SCALE * v.powf(1.0 / SRGB_EXPONENT) - SRGB_OFFSET
    }
}

/// ホストの CPU の画像（RGBA f32、premultiplied、sRGB の区分の式で線形にした値）を、Rec.1886 の式の線形に直す。
/// ホストの式は文字（`mm-text-cef`）と共有するので、web.cef の側で符号化値に戻してから Rec.1886 の式を適用し直す。
pub fn rec1886_from_srgb_linear(data: &mut [f32]) {
    for px in data.chunks_exact_mut(4) {
        let a = px[3];
        if a <= 0.0 {
            px.fill(0.0);
            continue;
        }
        for c in &mut px[..3] {
            let code = linear_to_srgb((*c / a).min(1.0));
            *c = code.powf(GAMMA) * a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb_to_linear(s: f32) -> f32 {
        if s <= 0.040_45 {
            s / SRGB_SLOPE
        } else {
            ((s + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_EXPONENT)
        }
    }

    #[test]
    fn 符号化値は_rec1886_の式の線形になる() {
        for code in [0u8, 6, 9, 13, 16, 64, 128, 200, 255] {
            let s = f32::from(code) / 255.0;
            let lin = srgb_to_linear(s);
            let mut px = [lin, lin, lin, 1.0];
            rec1886_from_srgb_linear(&mut px);
            let want = s.powf(GAMMA);
            assert!(
                (px[0] - want).abs() <= want * 1e-4 + 1e-7,
                "{code}: {} と {want}",
                px[0]
            );
        }
    }

    #[test]
    fn 半透明は割ってから直して掛け直す() {
        let a = 0.5;
        let s: f32 = 6.0 / 255.0;
        let mut px = [srgb_to_linear(s) * a, 0.0, a, a];
        rec1886_from_srgb_linear(&mut px);
        assert!((px[0] - s.powf(GAMMA) * a).abs() < 1e-7, "{}", px[0]);
        assert_eq!(px[1], 0.0);
        assert!((px[2] - a).abs() < 1e-6, "白は白のまま: {}", px[2]);
        assert_eq!(px[3], a);
    }

    #[test]
    fn 透明な画素は全部が_0() {
        let mut px = [0.3, 0.3, 0.3, 0.0];
        rec1886_from_srgb_linear(&mut px);
        assert_eq!(px, [0.0; 4]);
    }
}
