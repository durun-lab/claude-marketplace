//! WGSL を GPU で実行して CPU の値と突き合わせる。`--features gpu-eval` のときだけ動き、
//! アダプターが無ければスキップする。

use mm_color_api::{ColorTransform, TransformProgram};
use mm_color_lut::gpu_eval::GpuEval;
use mm_color_lut::{
    bake_lut3d, lut3d_apply_wgsl, sample_lut3d, sample_trilinear, Cdl, LutColor, LUT3D_APPLY_WGSL,
};
use mm_testkit::hdr::{self, HdrKind};
use mm_types::{tonemap, transfer, ColorSpace};

/// テスト用の色。黒、18% グレー、白、原色、彩度の高い色。
const SAMPLES: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0],
    [0.18, 0.18, 0.18],
    [1.0, 1.0, 1.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.8, 0.4, 0.1],
    [0.02, 0.5, 0.97],
];

fn max_diff(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs()))
        .fold(0.0, f32::max)
}

/// 1 を超える値（PQ の 1.0 は線形で約 49）は相対の差、1 以下は絶対の差の最大。GPU の `pow` の誤差は値に比例する。
fn max_scaled_diff(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| {
            x.iter()
                .zip(y)
                .map(|(p, q)| (p - q).abs() / q.abs().max(1.0))
        })
        .fold(0.0, f32::max)
}

#[test]
fn 伝達関数のwgslがcpuと一致する() {
    let Some(gpu) = GpuEval::new() else {
        eprintln!("アダプターが無いのでスキップする");
        return;
    };
    let ct = LutColor;
    let spaces = ct.known_spaces();
    for from in &spaces {
        for to in &spaces {
            let TransformProgram::Wgsl { source } = ct.program(from, to).unwrap() else {
                continue;
            };
            let got = gpu.run(&source, &SAMPLES, None).expect("GPU で実行");
            let want: Vec<_> = SAMPLES
                .iter()
                .map(|&c| ct.apply(from, to, c).unwrap())
                .collect();
            let d = max_scaled_diff(&got, &want);
            assert!(d < 1e-4, "{} -> {}: 差 {d}", from.0, to.0);
        }
    }
}

#[test]
fn cdlのwgslがcpuと一致する() {
    let Some(gpu) = GpuEval::new() else {
        return;
    };
    let cdl = Cdl {
        slope: [1.2, 0.9, 1.0],
        offset: [0.01, -0.02, 0.0],
        power: [1.1, 0.95, 1.0],
        saturation: 0.8,
    };
    let got = gpu.run(&cdl.to_wgsl(), &SAMPLES, None).expect("GPU で実行");
    let want: Vec<_> = SAMPLES.iter().map(|&c| cdl.apply(c)).collect();
    assert!(max_diff(&got, &want) < 1e-4);
}

#[test]
fn lutのwgslがcpuの補間と一致する() {
    let Some(gpu) = GpuEval::new() else {
        return;
    };
    let TransformProgram::Lut3d { size, data, .. } =
        bake_lut3d(17, |c| Ok(c.map(|v| v.powf(2.2)))).unwrap()
    else {
        panic!("Lut3d のはず");
    };
    let got = gpu
        .run(LUT3D_APPLY_WGSL, &SAMPLES, Some((size, &data)))
        .expect("GPU で実行");
    let want: Vec<_> = SAMPLES
        .iter()
        .map(|&c| sample_trilinear(size, &data, c))
        .collect();
    assert!(max_diff(&got, &want) < 1e-4);
}

#[test]
fn シェーパーつきのlutのwgslがcpuと一致し_1を超える値を切らない() {
    let Some(gpu) = GpuEval::new() else {
        return;
    };
    let ct = LutColor;
    let (from, to) = (ColorSpace::working(), ColorSpace::bt2100_pq());
    let TransformProgram::Lut3d { size, data, shaper } = ct.bake(&from, &to, 65).unwrap() else {
        panic!("Lut3d のはず");
    };
    let highlights = [[4.0, 2.0, 0.5], [10.0, 10.0, 10.0], [0.18, 1.5, 3.0]];
    let inputs: Vec<[f32; 3]> = SAMPLES.iter().chain(&highlights).copied().collect();
    let source = format!("{}{}", transfer::TRANSFER_WGSL, lut3d_apply_wgsl(shaper));
    let got = gpu
        .run(&source, &inputs, Some((size, &data)))
        .expect("GPU で実行");
    let cpu: Vec<_> = inputs
        .iter()
        .map(|&c| sample_lut3d(size, &data, shaper, c))
        .collect();
    assert!(max_diff(&got, &cpu) < 1e-4, "GPU と CPU の補間");
    let exact: Vec<_> = inputs
        .iter()
        .map(|&c| ct.apply(&from, &to, c).unwrap())
        .collect();
    let d = max_diff(&cpu, &exact);
    eprintln!("PQ のシェーパーつき 65^3 の LUT と式の最大差 {d:e}");
    assert!(d < 2e-3, "1 を超える線形の値も格子の中で引ける: {d}");
}

/// プレビュー（`mm-ui-wgpu`）と GPU の NV12 の書き出し（`mm-render`）が使う `TONEMAP_WGSL` が、CPU の書き出し
/// （`mm-encode-ffmpeg-core`）の `bt2390_to_sdr` と同じ値を出す。入力は HDR の素材のフィクスチャの帯の線形の値と、
/// 色の見本と、素材の上限を超える値。
#[test]
fn sdr_のトーンマップのwgslがcpuと一致する() {
    let Some(gpu) = GpuEval::new() else {
        return;
    };
    let bands = (0..hdr::BANDS.len()).flat_map(|band| {
        [
            hdr::expected(HdrKind::Pq, band),
            hdr::expected(HdrKind::Hlg, band),
        ]
    });
    let over = [[8.0, 6.0, 0.5], [30.0, 30.0, 30.0]];
    let inputs: Vec<[f32; 3]> = SAMPLES.iter().copied().chain(bands).chain(over).collect();
    let peak = tonemap::preview_peak(&[ColorSpace::bt2100_pq()]);
    for peak in [1.0, peak] {
        let source = format!(
            "{}{}fn mm_color_transform(c: vec3<f32>) -> vec3<f32> {{ return mm_tonemap_sdr(c, {peak:?}); }}",
            transfer::TRANSFER_WGSL,
            tonemap::TONEMAP_WGSL
        );
        let got = gpu.run(&source, &inputs, None).expect("GPU で実行");
        let want: Vec<_> = inputs
            .iter()
            .map(|&c| tonemap::bt2390_to_sdr(c, peak))
            .collect();
        let d = max_diff(&got, &want);
        eprintln!("上限 {peak} の SDR のトーンマップの GPU と CPU の最大差 {d:e}");
        assert!(d < 1e-4, "上限 {peak}: 差 {d}");
    }
}
