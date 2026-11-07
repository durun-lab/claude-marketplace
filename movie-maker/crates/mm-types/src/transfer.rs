//! 色の符号化値と線形の間の式（伝達関数）。CPU の実装と、同じ式の WGSL を 1 か所に置き、全経路がここを使う。
//!
//! 8 bit の色（素材の画素、CSS、theme、文字の色、絵文字）は Rec.1886 の符号化値として線形にする。デコーダーと書き出しが
//! この式なので、16 進の色は書き出しで同じ符号化値に戻る（DaVinci や Premiere が sRGB の静止画を Rec.709 のタイムラインに
//! 値のまま置くのと同じ考え方）。sRGB の区分の式は、色空間 `srgb` を名指しした変換だけが使う。
//! 広色域（`bt2020`）の色は Rec.709 の線形で負になるので、どの関数も負の値を原点対称に折り返す（`sign(v) * f(abs(v))`）。

/// Rec.1886 の表示の式のべき。WGSL の [`TRANSFER_WGSL`] にも同じ値を書く。
pub const BT1886_GAMMA: f32 = 2.4;

/// 8 bit の符号化値の最大。
const MAX8: f32 = 255.0;
/// sRGB の区分の式（IEC 61966-2-1）の折れ目（符号化値の側と線形の側）、直線部の傾き、曲線部の係数とべき。
const SRGB_ENCODED_KNEE: f32 = 0.040_45;
const SRGB_LINEAR_KNEE: f32 = 0.003_130_8;
const SRGB_SLOPE: f32 = 12.92;
const SRGB_SCALE: f32 = 1.055;
const SRGB_OFFSET: f32 = 0.055;
const SRGB_GAMMA: f32 = 2.4;
/// BT.709 のカメラの OETF の折れ目（符号化値の側と線形の側）、直線部の傾き、曲線部の係数とべき。
const BT709_ENCODED_KNEE: f32 = 0.081;
const BT709_LINEAR_KNEE: f32 = 0.018;
const BT709_SLOPE: f32 = 4.5;
const BT709_SCALE: f32 = 1.099;
const BT709_OFFSET: f32 = 0.099;
const BT709_POWER: f32 = 0.45;

/// Rec.1886 の表示の式。符号化値から線形へ（べき 2.4、負は符号を保つ）。
pub fn bt1886_to_linear(v: f32) -> f32 {
    v.signum() * v.abs().powf(BT1886_GAMMA)
}

/// Rec.1886 の表示の式の逆。線形から符号化値へ。
pub fn linear_to_bt1886(v: f32) -> f32 {
    v.signum() * v.abs().powf(1.0 / BT1886_GAMMA)
}

/// 8 bit の符号化値を線形にする（Rec.1886）。
pub fn code8_to_linear(code: u8) -> f32 {
    bt1886_to_linear(f32::from(code) / MAX8)
}

/// 線形の値を 8 bit の符号化値に丸める（Rec.1886）。範囲外は端に丸め、NaN は 0。
pub fn linear_to_code8(v: f32) -> u8 {
    if v.is_nan() {
        return 0;
    }
    // 0..=1 に切ってから 255 倍して丸めるので u8 に収まる
    (linear_to_bt1886(v.clamp(0.0, 1.0)) * MAX8).round() as u8
}

/// 8 bit の符号化値 0..=255 → 線形の表。全画素で `powf` を呼ばないためのルックアップテーブル。
pub fn code8_table() -> [f32; 256] {
    std::array::from_fn(|code| bt1886_to_linear(code as f32 / MAX8))
}

/// sRGB の符号化値から線形へ（IEC 61966-2-1 の区分の式）。色空間 `srgb` の変換だけが使う。
pub fn srgb_to_linear(v: f32) -> f32 {
    let a = v.abs();
    let l = if a <= SRGB_ENCODED_KNEE {
        a / SRGB_SLOPE
    } else {
        ((a + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_GAMMA)
    };
    v.signum() * l
}

/// 線形から sRGB の符号化値へ。色空間 `srgb` の変換だけが使う。
pub fn linear_to_srgb(v: f32) -> f32 {
    let a = v.abs();
    let e = if a <= SRGB_LINEAR_KNEE {
        a * SRGB_SLOPE
    } else {
        SRGB_SCALE * a.powf(1.0 / SRGB_GAMMA) - SRGB_OFFSET
    };
    v.signum() * e
}

/// BT.709 のカメラの OETF の逆（`bt709_oetf`）。符号化値から線形へ。
pub fn bt709_to_linear(v: f32) -> f32 {
    let a = v.abs();
    let l = if a < BT709_ENCODED_KNEE {
        a / BT709_SLOPE
    } else {
        ((a + BT709_OFFSET) / BT709_SCALE).powf(1.0 / BT709_POWER)
    };
    v.signum() * l
}

/// BT.709 の OETF。線形から符号化値へ。
pub fn linear_to_bt709(v: f32) -> f32 {
    let a = v.abs();
    let e = if a < BT709_LINEAR_KNEE {
        a * BT709_SLOPE
    } else {
        BT709_SCALE * a.powf(BT709_POWER) - BT709_OFFSET
    };
    v.signum() * e
}

/// WGSL の伝達関数。シェーダーの先頭に連結して使う。CPU 側と同じ式で、負の値は符号を保って折り返す。
/// `vec3` の版（`*3`）は RGB をまとめて変換する。
pub const TRANSFER_WGSL: &str = r"
fn mm_srgb_to_linear(v: f32) -> f32 {
    let s = sign(v);
    let a = abs(v);
    if a <= 0.04045 { return s * a / 12.92; }
    return s * pow((a + 0.055) / 1.055, 2.4);
}

fn mm_linear_to_srgb(v: f32) -> f32 {
    let s = sign(v);
    let a = abs(v);
    if a <= 0.0031308 { return s * a * 12.92; }
    return s * (1.055 * pow(a, 1.0 / 2.4) - 0.055);
}

fn mm_bt1886_to_linear(v: f32) -> f32 {
    return sign(v) * pow(abs(v), 2.4);
}

fn mm_linear_to_bt1886(v: f32) -> f32 {
    return sign(v) * pow(abs(v), 1.0 / 2.4);
}

fn mm_bt709_to_linear(v: f32) -> f32 {
    let s = sign(v);
    let a = abs(v);
    if a < 0.081 { return s * a / 4.5; }
    return s * pow((a + 0.099) / 1.099, 1.0 / 0.45);
}

fn mm_linear_to_bt709(v: f32) -> f32 {
    let s = sign(v);
    let a = abs(v);
    if a < 0.018 { return s * a * 4.5; }
    return s * (1.099 * pow(a, 0.45) - 0.099);
}

fn mm_srgb_to_linear3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_srgb_to_linear(c.x), mm_srgb_to_linear(c.y), mm_srgb_to_linear(c.z));
}

fn mm_linear_to_srgb3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_linear_to_srgb(c.x), mm_linear_to_srgb(c.y), mm_linear_to_srgb(c.z));
}

fn mm_bt1886_to_linear3(c: vec3<f32>) -> vec3<f32> {
    return sign(c) * pow(abs(c), vec3<f32>(2.4));
}

fn mm_linear_to_bt1886_3(c: vec3<f32>) -> vec3<f32> {
    return sign(c) * pow(abs(c), vec3<f32>(1.0 / 2.4));
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 八ビットの符号化値は往復で同じ値に戻る() {
        for code in 0..=u8::MAX {
            assert_eq!(linear_to_code8(code8_to_linear(code)), code);
        }
        assert_eq!(code8_table()[128], code8_to_linear(128));
        assert!((code8_to_linear(128) - 0.1913).abs() < 1e-3);
    }

    #[test]
    fn 範囲外は端に丸める() {
        assert_eq!(linear_to_code8(-1.0), 0);
        assert_eq!(linear_to_code8(2.0), 255);
        assert_eq!(linear_to_code8(f32::NAN), 0);
    }

    #[test]
    fn 暗部は_srgb_の区分の式より暗い() {
        // `#06090d` の 6 は Rec.1886 では sRGB の区分の式の 1/10 ほどの光になる
        let s = 6.0 / MAX8;
        assert!(bt1886_to_linear(s) < srgb_to_linear(s) * 0.1);
    }

    #[test]
    fn 各式は往復する() {
        type Curve = fn(f32) -> f32;
        let pairs: [(Curve, Curve); 3] = [
            (srgb_to_linear, linear_to_srgb),
            (bt1886_to_linear, linear_to_bt1886),
            (bt709_to_linear, linear_to_bt709),
        ];
        for (decode, encode) in pairs {
            for v in [-0.5, 0.0, 0.01, 0.04, 0.2, 0.5, 1.0] {
                assert!((encode(decode(v)) - v).abs() < 1e-5, "{v}");
            }
        }
    }

    #[test]
    fn wgsl_のべきは_cpu_と同じ() {
        assert!(TRANSFER_WGSL.contains(&format!("pow(abs(v), {BT1886_GAMMA:?})")));
    }
}
