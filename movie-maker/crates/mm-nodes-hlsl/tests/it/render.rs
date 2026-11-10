//! HLSL のフィクスチャで描いた画素が、同じことをする WGSL（`shader.wgsl`、`transition.wgsl`）と一致すること。

use mm_node_api::NodeKind;
use mm_nodes_basic::{ShaderWgslKind, TransitionWgslKind};
use mm_nodes_hlsl::{ShaderHlslKind, ASSET_PARAM, CODE_PARAM};
use mm_testkit::gpu::CpuGpu;
use mm_types::{AssetId, MmError, ParamValue, RationalTime, RgbaImage, Size};

use crate::common::{
    fixture, fixture_path, gpu_and_dxc, gradient, params_of, render_image, render_with,
};

const SIZE: Size = Size::new(48, 27);
/// 同じ GPU で同じ式を描くので、差は f16 の丸めの 1 段まで。
const TOLERANCE: f32 = 1e-3;

const INVERT_WGSL: &str = "fn mm_main(uv: vec2<f32>) -> vec4<f32> {
    let c = mm_input(uv);
    return vec4<f32>(c.a - c.rgb, c.a);
}";

const MIX_WGSL: &str = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {
    return mix(mm_from(uv), mm_to(uv), u.progress);
}";

const TIME_WGSL: &str = "fn mm_main(uv: vec2<f32>) -> vec4<f32> {
    let w = 0.5 + 0.5 * sin((uv.x - u.time * 0.25) * 18.0);
    let c = mix(vec3<f32>(0.02, 0.03, 0.1), vec3<f32>(0.9, 0.5, 0.1), w);
    return vec4<f32>(c * (0.5 + 0.5 * uv.y), 1.0);
}";

fn text(s: &str) -> ParamValue {
    ParamValue::Text(s.into())
}

fn millis(ms: i128) -> RationalTime {
    RationalTime::new(ms, 1000).expect("有効な時刻")
}

fn assert_same(hlsl: &RgbaImage, wgsl: &RgbaImage, what: &str) {
    let diff = hlsl.max_diff(wgsl).expect("同じ大きさ");
    assert!(diff <= TOLERANCE, "{what}: HLSL と WGSL の差 {diff}");
}

#[test]
fn 色の反転は_shader_wgsl_と画素が一致する() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let hlsl = ShaderHlslKind::with_dxc(Some(dxc));
    let wgsl = ShaderWgslKind::default();
    let input = || vec![gradient(SIZE, false)];
    let at = (SIZE, RationalTime::seconds(1));
    let a = render_image(
        &ctx,
        &hlsl,
        &params_of(&hlsl, &[(CODE_PARAM, text(&fixture("invert.hlsl")))]),
        input(),
        at,
    );
    let b = render_image(
        &ctx,
        &wgsl,
        &params_of(&wgsl, &[("code", text(INVERT_WGSL))]),
        input(),
        at,
    );
    assert_same(&a, &b, "反転");
    let corner = a.pixel(SIZE.width - 1, SIZE.height - 1);
    assert!(
        corner.a > 0.9 && corner.b > 0.0,
        "右下は不透明で反転した色: {corner:?}"
    );
}

#[test]
fn 二つの入力を混ぜると_transition_wgsl_と画素が一致する() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let hlsl = ShaderHlslKind::with_dxc(Some(dxc));
    let wgsl = TransitionWgslKind::default();
    let inputs = || vec![gradient(SIZE, false), gradient(SIZE, true)];
    let at = (SIZE, RationalTime::seconds(1));
    let p = params_of(
        &hlsl,
        &[
            (CODE_PARAM, text(&fixture("mix.hlsl"))),
            ("p0", ParamValue::Number(0.3)),
        ],
    );
    let q = params_of(
        &wgsl,
        &[
            ("code", text(MIX_WGSL)),
            ("progress", ParamValue::Number(0.3)),
        ],
    );
    assert_same(
        &render_image(&ctx, &hlsl, &p, inputs(), at),
        &render_image(&ctx, &wgsl, &q, inputs(), at),
        "混ぜる",
    );
}

#[test]
fn 時刻で動く縞は各時刻で_shader_wgsl_と画素が一致する() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let hlsl = ShaderHlslKind::with_dxc(Some(dxc));
    let wgsl = ShaderWgslKind::default();
    let p = params_of(&hlsl, &[(CODE_PARAM, text(&fixture("time.hlsl")))]);
    let q = params_of(&wgsl, &[("code", text(TIME_WGSL))]);
    let mut frames = Vec::new();
    for t in [millis(0), millis(1500)] {
        let a = render_image(&ctx, &hlsl, &p, Vec::new(), (SIZE, t));
        assert_same(
            &a,
            &render_image(&ctx, &wgsl, &q, Vec::new(), (SIZE, t)),
            "時刻",
        );
        frames.push(a);
    }
    assert!(
        frames[0].max_diff(&frames[1]).expect("同じ大きさ") > 0.1,
        "時刻で絵が変わる"
    );
}

#[test]
fn 源の素材はレンダラーのデコーダーに渡さない() {
    // 真だとレンダラーが .hlsl をデコーダーで開こうとして、そのレイヤーが描けない。
    assert!(!ShaderHlslKind::default().decodes_asset());
}

#[test]
fn 素材のファイルの源でも描ける() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let hlsl = ShaderHlslKind::with_dxc(Some(dxc));
    let asset = fixture_path("invert.hlsl").to_string_lossy().into_owned();
    let from_asset = params_of(
        &hlsl,
        &[(ASSET_PARAM, ParamValue::AssetRef(AssetId::new(asset)))],
    );
    let from_code = params_of(&hlsl, &[(CODE_PARAM, text(&fixture("invert.hlsl")))]);
    let at = (SIZE, RationalTime::seconds(1));
    let a = render_image(&ctx, &hlsl, &from_asset, vec![gradient(SIZE, false)], at);
    let b = render_image(&ctx, &hlsl, &from_code, vec![gradient(SIZE, false)], at);
    assert_eq!(a.max_diff(&b), Some(0.0));
}

#[test]
fn dxc_が無ければそのレイヤーだけ_unsupported() {
    let Some(ctx) = crate::common::gpu() else {
        return;
    };
    let kind = ShaderHlslKind::with_dxc(None);
    let mut wgpu = mm_gpu::WgpuGpu::new(ctx);
    let params = kind.params().defaults();
    let err = render_with(
        &kind,
        &params,
        &mut wgpu,
        Vec::new(),
        (SIZE, RationalTime::ZERO),
    )
    .unwrap_err();
    assert!(
        matches!(err, MmError::Unsupported(ref m) if m.contains("DXC")),
        "{err:?}"
    );
}

#[test]
fn gpu_が無ければ_unsupported() {
    let kind = ShaderHlslKind::with_dxc(None);
    let params = kind.params().defaults();
    let err = render_with(
        &kind,
        &params,
        &mut CpuGpu::new(),
        Vec::new(),
        (SIZE, RationalTime::ZERO),
    )
    .unwrap_err();
    assert!(matches!(err, MmError::Unsupported(_)), "{err:?}");
}

#[test]
fn 入力の大きさが違えば誤り() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let hlsl = ShaderHlslKind::with_dxc(Some(dxc));
    let p = params_of(&hlsl, &[(CODE_PARAM, text(&fixture("mix.hlsl")))]);
    let mut wgpu = mm_gpu::WgpuGpu::new(ctx);
    let inputs = vec![gradient(SIZE, false), gradient(Size::new(8, 8), false)];
    let err = render_with(&hlsl, &p, &mut wgpu, inputs, (SIZE, RationalTime::ZERO)).unwrap_err();
    assert!(matches!(err, MmError::InvalidArgument(_)), "{err:?}");
}

#[test]
fn 素材の_hlsl_を書き換えると指紋が変わり_読めなければキャッシュしない() {
    let guard = tempfile::Builder::new()
        .prefix("mm-nodes-hlsl-fp-")
        .tempdir()
        .unwrap();
    let dir = guard.path().to_path_buf();
    let path = dir.join("a.hlsl");
    std::fs::write(&path, fixture("invert.hlsl")).expect("書ける");
    let kind = ShaderHlslKind::with_dxc(None);
    let p = params_of(
        &kind,
        &[(ASSET_PARAM, ParamValue::AssetRef(AssetId::new("src")))],
    );
    let fp = |p: &mm_types::Params| kind.cache_fingerprint(p, &|_| Some(path.clone()));
    let first = fp(&p).expect("読める");
    assert!(first.starts_with("dxc=none|"), "{first}");
    std::fs::write(&path, fixture("mix.hlsl")).expect("書ける");
    assert_ne!(fp(&p), Some(first), "中身が変われば別の指紋");
    std::fs::remove_file(&path).expect("消せる");
    assert_eq!(fp(&p), None, "読めない素材はキャッシュしない");
    let code = params_of(&kind, &[(CODE_PARAM, text(&fixture("invert.hlsl")))]);
    assert_eq!(
        fp(&code).as_deref(),
        Some("dxc=none"),
        "源がパラメータなら DXC だけ"
    );
}
