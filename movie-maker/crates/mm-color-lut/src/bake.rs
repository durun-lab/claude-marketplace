//! 任意の変換を 3D LUT にベイクする。OCIO の GPU のシェーダーが使えない（テクスチャを持つ）ときのフォールバック。

use mm_color_api::{LutShaper, TransformProgram};
use mm_types::Result;

/// 格子の各点で `f` を呼び、`TransformProgram::Lut3d`（R が最も速く変わる）にする。
/// 格子の入力は 0..=1 を `size` 等分した点。
pub fn bake_lut3d(
    size: u32,
    f: impl FnMut([f32; 3]) -> Result<[f32; 3]>,
) -> Result<TransformProgram> {
    bake_lut3d_shaped(size, LutShaper::None, f)
}

/// [`bake_lut3d`] に 1D の曲線を前置きしたもの。格子の各点を `shaper` の逆で入力の値に戻してから `f` を呼ぶので、
/// `LutShaper::Pq` なら 1 を超える線形の光も格子の中に入る。
pub fn bake_lut3d_shaped(
    size: u32,
    shaper: LutShaper,
    mut f: impl FnMut([f32; 3]) -> Result<[f32; 3]>,
) -> Result<TransformProgram> {
    let n = size as usize;
    let step = (n - 1) as f32;
    let mut data = Vec::with_capacity(n * n * n * 3);
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let rgb = [r, g, b].map(|v| shaper.invert(v as f32 / step));
                data.extend_from_slice(&f(rgb)?);
            }
        }
    }
    Ok(TransformProgram::Lut3d { size, data, shaper })
}

/// `Lut3d` を 1 画素に適用する（CPU）。`shaper` を掛けてから三線形補間で引く。
pub fn sample_lut3d(size: u32, data: &[f32], shaper: LutShaper, rgb: [f32; 3]) -> [f32; 3] {
    crate::cube::sample_trilinear(size, data, rgb.map(|c| shaper.apply(c)))
}

/// `Lut3d` を引く WGSL。[`LUT3D_APPLY_WGSL`] の入力に `shaper` を前置きした `mm_color_transform` を返す。
/// `mm_types::transfer::TRANSFER_WGSL` を前に連結する。
pub fn lut3d_apply_wgsl(shaper: LutShaper) -> String {
    let body = LUT3D_APPLY_WGSL.replace(
        "fn mm_color_transform(c: vec3<f32>)",
        "fn mm_lut3d_lookup(c: vec3<f32>)",
    );
    format!(
        "{body}\nfn mm_color_transform(c: vec3<f32>) -> vec3<f32> {{ return mm_lut3d_lookup({}); }}\n",
        shaper.wgsl("c")
    )
}

/// `Lut3d` を引く WGSL。バインドは `@group(1) @binding(0)` の `texture_3d<f32>`（rgba32float で
/// 一辺 `size`）。サンプラーを使わず `textureLoad` で三線形補間するので、フィルタ非対応の形式でも動く。
pub const LUT3D_APPLY_WGSL: &str = r"
@group(1) @binding(0) var mm_lut: texture_3d<f32>;

fn mm_lut_at(i: vec3<i32>) -> vec3<f32> {
    return textureLoad(mm_lut, i, 0).rgb;
}

fn mm_color_transform(c: vec3<f32>) -> vec3<f32> {
    let n = i32(textureDimensions(mm_lut).x);
    let p = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * f32(n - 1);
    let fl = floor(p);
    let f = p - fl;
    let i0 = vec3<i32>(fl);
    let i1 = min(i0 + vec3<i32>(1), vec3<i32>(n - 1));
    let c000 = mm_lut_at(vec3<i32>(i0.x, i0.y, i0.z));
    let c100 = mm_lut_at(vec3<i32>(i1.x, i0.y, i0.z));
    let c010 = mm_lut_at(vec3<i32>(i0.x, i1.y, i0.z));
    let c110 = mm_lut_at(vec3<i32>(i1.x, i1.y, i0.z));
    let c001 = mm_lut_at(vec3<i32>(i0.x, i0.y, i1.z));
    let c101 = mm_lut_at(vec3<i32>(i1.x, i0.y, i1.z));
    let c011 = mm_lut_at(vec3<i32>(i0.x, i1.y, i1.z));
    let c111 = mm_lut_at(vec3<i32>(i1.x, i1.y, i1.z));
    let x00 = mix(c000, c100, f.x);
    let x10 = mix(c010, c110, f.x);
    let x01 = mix(c001, c101, f.x);
    let x11 = mix(c011, c111, f.x);
    return mix(mix(x00, x10, f.y), mix(x01, x11, f.y), f.z);
}
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::sample_trilinear;

    #[test]
    fn ベイクしたlutは元の変換に近い() {
        let f = |c: [f32; 3]| Ok(c.map(|v| v * v));
        let TransformProgram::Lut3d { size, data, .. } = bake_lut3d(33, f).unwrap() else {
            panic!("Lut3d のはず");
        };
        let out = sample_trilinear(size, &data, [0.5, 0.25, 0.75]);
        for (a, b) in out.iter().zip([0.25, 0.0625, 0.5625]) {
            assert!((a - b).abs() < 1e-3, "{a} {b}");
        }
    }

    #[test]
    fn pq_のシェーパーなら_1_を超える線形の光も切られない() {
        let f = |c: [f32; 3]| Ok(c.map(|v| v * 0.5));
        let TransformProgram::Lut3d { size, data, shaper } =
            bake_lut3d_shaped(33, LutShaper::Pq, f).unwrap()
        else {
            panic!("Lut3d のはず");
        };
        for v in [0.01_f32, 0.18, 1.0, 4.0, 20.0] {
            let out = sample_lut3d(size, &data, shaper, [v; 3]);
            assert!((out[0] / (v * 0.5) - 1.0).abs() < 0.02, "{v}: {out:?}");
        }
        let plain = bake_lut3d(33, f).unwrap();
        let TransformProgram::Lut3d { size, data, shaper } = plain else {
            panic!("Lut3d のはず");
        };
        assert!(
            sample_lut3d(size, &data, shaper, [4.0; 3])[0] <= 0.5 + 1e-6,
            "シェーパーなしは 1 で切る"
        );
    }
}
