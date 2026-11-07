//! 色の符号化値と線形の間の式（伝達関数）。CPU の実装と、同じ式の WGSL を 1 か所に置き、全経路がここを使う。
//!
//! 8 bit の色（素材の画素、CSS、theme、文字の色、絵文字）は Rec.1886 の符号化値として線形にする。デコーダーと書き出しが
//! この式なので、16 進の色は書き出しで同じ符号化値に戻る（DaVinci や Premiere が sRGB の静止画を Rec.709 のタイムラインに
//! 値のまま置くのと同じ考え方）。sRGB の区分の式は、色空間 `srgb` を名指しした変換だけが使う。
//! 広色域（`bt2020`）の色は Rec.709 の線形で負になるので、どの関数も負の値を原点対称に折り返す（`sign(v) * f(abs(v))`）。
//!
//! HDR（PQ と HLG）の線形は、1.0 を SDR の白（BT.2408 の HDR の基準の白、203 cd/m²）に置く。SDR の文字や画像を
//! HDR の素材と同じ作業色空間で重ねても、白が HDR の基準の白に揃う。1 を超える値はハイライトで、切らずに持ち回る。

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

/// 線形の 1.0 の明るさ（cd/m²）。BT.2408 の HDR の基準の白。WGSL の [`TRANSFER_WGSL`] にも同じ値を書く。
pub const HDR_REFERENCE_WHITE_NITS: f32 = 203.0;
/// PQ の符号化値 1.0 の明るさ（cd/m²）。
pub const PQ_PEAK_NITS: f32 = 10_000.0;
/// HLG の表示の明るさの上限（cd/m²）。BT.2100 の基準の表示（システムガンマ 1.2）。
pub const HLG_PEAK_NITS: f32 = 1_000.0;
/// SMPTE ST 2084 の係数。
const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f32 = 2392.0 / 4096.0 * 32.0;
/// BT.2100 の HLG の OETF の係数と、直線部（平方根）と対数部の境（符号化値の側とシーンの光の側）。
const HLG_A: f32 = 0.178_832_77;
const HLG_B: f32 = 0.284_668_92;
const HLG_C: f32 = 0.559_910_7;
const HLG_ENCODED_KNEE: f32 = 0.5;
const HLG_SCENE_KNEE: f32 = 1.0 / 12.0;
/// HLG の OOTF のシステムガンマ（表示の上限 1000 cd/m² のとき）。
const HLG_SYSTEM_GAMMA: f32 = 1.2;

/// 線形の Rec.2020（D65）を線形の Rec.709 にする 3x3 行列（BT.2407）。行ごとの和は 1 で、白は白のまま。
/// 結果は 0..1 の外に出る（広色域の色は Rec.709 では負になる）ので、切らずに持ち回る。
pub const REC2020_TO_REC709: [[f32; 3]; 3] = [
    [1.660_491, -0.587_641, -0.072_85],
    [-0.124_55, 1.132_9, -0.008_349],
    [-0.018_151, -0.100_579, 1.118_73],
];

/// 線形の Rec.709 を線形の Rec.2020 にする 3x3 行列。[`REC2020_TO_REC709`] の逆。
pub const REC709_TO_REC2020: [[f32; 3]; 3] = [
    [0.627_404, 0.329_283, 0.043_313],
    [0.069_097, 0.919_540, 0.011_362],
    [0.016_391, 0.088_013, 0.895_595],
];

/// 3x3 行列を RGB に掛ける。
pub fn mul3(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// PQ（SMPTE ST 2084）の符号化値から線形へ（1.0 が [`HDR_REFERENCE_WHITE_NITS`]）。
pub fn pq_to_linear(v: f32) -> f32 {
    let p = v.abs().powf(1.0 / PQ_M2);
    let y = ((p - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * p)).powf(1.0 / PQ_M1);
    v.signum() * y * (PQ_PEAK_NITS / HDR_REFERENCE_WHITE_NITS)
}

/// 線形から PQ の符号化値へ。10000 cd/m² を超える値は 1 を超える符号化値になる（切るのは書き出しの側）。
pub fn linear_to_pq(v: f32) -> f32 {
    if v == 0.0 {
        return 0.0;
    }
    let y = (v.abs() * (HDR_REFERENCE_WHITE_NITS / PQ_PEAK_NITS)).powf(PQ_M1);
    v.signum() * ((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y)).powf(PQ_M2)
}

/// HLG（BT.2100）の符号化値から線形へ。OETF の逆で戻したシーンの光に、表示の上限 1000 cd/m² の OOTF
/// （システムガンマ 1.2）を掛ける。OOTF は輝度ではなくチャンネルごとに掛け、無彩色では BT.2100 の式と一致する。
pub fn hlg_to_linear(v: f32) -> f32 {
    let a = v.abs();
    let scene = if a <= HLG_ENCODED_KNEE {
        a * a / 3.0
    } else {
        (((a - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    };
    v.signum() * scene.powf(HLG_SYSTEM_GAMMA) * (HLG_PEAK_NITS / HDR_REFERENCE_WHITE_NITS)
}

/// 線形から HLG の符号化値へ。[`hlg_to_linear`] の逆。
pub fn linear_to_hlg(v: f32) -> f32 {
    let scene = (v.abs() * (HDR_REFERENCE_WHITE_NITS / HLG_PEAK_NITS)).powf(1.0 / HLG_SYSTEM_GAMMA);
    let e = if scene <= HLG_SCENE_KNEE {
        (3.0 * scene).sqrt()
    } else {
        HLG_A * (12.0 * scene - HLG_B).ln() + HLG_C
    };
    v.signum() * e
}

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

fn mm_pq_to_linear(v: f32) -> f32 {
    let p = pow(abs(v), 1.0 / 78.84375);
    let y = pow(max(p - 0.8359375, 0.0) / (18.8515625 - 18.6875 * p), 1.0 / 0.1593017578125);
    return sign(v) * y * (10000.0 / 203.0);
}

fn mm_linear_to_pq(v: f32) -> f32 {
    let y = pow(abs(v) * (203.0 / 10000.0), 0.1593017578125);
    return sign(v) * pow((0.8359375 + 18.8515625 * y) / (1.0 + 18.6875 * y), 78.84375);
}

fn mm_hlg_to_linear(v: f32) -> f32 {
    let a = abs(v);
    var scene = a * a / 3.0;
    if a > 0.5 { scene = (exp((a - 0.55991073) / 0.17883277) + 0.28466892) / 12.0; }
    return sign(v) * pow(scene, 1.2) * (1000.0 / 203.0);
}

fn mm_linear_to_hlg(v: f32) -> f32 {
    let scene = pow(abs(v) * (203.0 / 1000.0), 1.0 / 1.2);
    var e = sqrt(3.0 * scene);
    if scene > 1.0 / 12.0 { e = 0.17883277 * log(12.0 * scene - 0.28466892) + 0.55991073; }
    return sign(v) * e;
}

fn mm_pq_to_linear3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_pq_to_linear(c.x), mm_pq_to_linear(c.y), mm_pq_to_linear(c.z));
}

fn mm_linear_to_pq3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_linear_to_pq(c.x), mm_linear_to_pq(c.y), mm_linear_to_pq(c.z));
}

fn mm_hlg_to_linear3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_hlg_to_linear(c.x), mm_hlg_to_linear(c.y), mm_hlg_to_linear(c.z));
}

fn mm_linear_to_hlg3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(mm_linear_to_hlg(c.x), mm_linear_to_hlg(c.y), mm_linear_to_hlg(c.z));
}

fn mm_rec2020_to_rec709(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(vec3<f32>(1.660491, -0.587641, -0.07285), c),
        dot(vec3<f32>(-0.12455, 1.1329, -0.008349), c),
        dot(vec3<f32>(-0.018151, -0.100579, 1.11873), c),
    );
}

fn mm_rec709_to_rec2020(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(vec3<f32>(0.627404, 0.329283, 0.043313), c),
        dot(vec3<f32>(0.069097, 0.91954, 0.011362), c),
        dot(vec3<f32>(0.016391, 0.088013, 0.895595), c),
    );
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
        let pairs: [(Curve, Curve); 5] = [
            (srgb_to_linear, linear_to_srgb),
            (bt1886_to_linear, linear_to_bt1886),
            (bt709_to_linear, linear_to_bt709),
            (pq_to_linear, linear_to_pq),
            (hlg_to_linear, linear_to_hlg),
        ];
        for (decode, encode) in pairs {
            for v in [-0.5, 0.0, 0.01, 0.04, 0.2, 0.5, 0.75, 1.0] {
                assert!((encode(decode(v)) - v).abs() < 1e-5, "{v}");
            }
        }
    }

    #[test]
    fn pq_は_st2084_の明るさになる() {
        let nits = |code: f32| pq_to_linear(code) * HDR_REFERENCE_WHITE_NITS;
        assert!((nits(1.0) - PQ_PEAK_NITS).abs() < 1.0, "{}", nits(1.0));
        // ST 2084 の表: 符号化値 0.5081 が 100 cd/m²、0.7518 が 1000 cd/m²
        assert!((nits(0.508_078) - 100.0).abs() < 0.1, "{}", nits(0.508_078));
        assert!(
            (nits(0.751_827) - 1000.0).abs() < 1.0,
            "{}",
            nits(0.751_827)
        );
        assert!(
            (linear_to_pq(1.0) - 0.580_7).abs() < 1e-3,
            "203 cd/m² の符号化値"
        );
        assert_eq!(linear_to_pq(0.0), 0.0);
    }

    #[test]
    fn hlg_の_75_パーセントは_sdr_の白になる() {
        // BT.2408: 1000 cd/m² の表示で、HLG の 75% が HDR の基準の白（203 cd/m²）
        assert!(
            (hlg_to_linear(0.75) - 1.0).abs() < 5e-3,
            "{}",
            hlg_to_linear(0.75)
        );
        let peak = hlg_to_linear(1.0) * HDR_REFERENCE_WHITE_NITS;
        assert!((peak - HLG_PEAK_NITS).abs() < 1.0, "{peak}");
        // 直線部（平方根）と対数部の境で連続
        let knee = HLG_ENCODED_KNEE;
        assert!((hlg_to_linear(knee - 1e-4) - hlg_to_linear(knee + 1e-4)).abs() < 1e-3);
    }

    #[test]
    fn 原色の行列は互いの逆で白を保つ() {
        let v = [0.3, 0.6, 0.1];
        let back = mul3(&REC709_TO_REC2020, mul3(&REC2020_TO_REC709, v));
        for (a, b) in v.iter().zip(back) {
            assert!((a - b).abs() < 1e-4, "{a} {b}");
        }
        for c in mul3(&REC2020_TO_REC709, [1.0; 3]) {
            assert!((c - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn wgsl_の係数は_cpu_と同じ() {
        assert!(TRANSFER_WGSL.contains(&format!("pow(abs(v), {BT1886_GAMMA:?})")));
        assert!(TRANSFER_WGSL.contains(&format!("{PQ_M2:?}")));
        assert!(TRANSFER_WGSL.contains(&format!(
            "({PQ_PEAK_NITS:.1} / {HDR_REFERENCE_WHITE_NITS:.1})"
        )));
        assert!(TRANSFER_WGSL.contains(&format!(
            "({HLG_PEAK_NITS:.1} / {HDR_REFERENCE_WHITE_NITS:.1})"
        )));
        for row in REC2020_TO_REC709.iter().chain(REC709_TO_REC2020.iter()) {
            assert!(TRANSFER_WGSL.contains(&format!("{:?}, {:?}, {:?}", row[0], row[1], row[2])));
        }
    }
}
