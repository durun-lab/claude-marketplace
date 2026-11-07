//! 変換（DXC → naga）の誤りの行番号と、源のハッシュのキャッシュ。

use std::time::Instant;

use mm_node_api::NodeKind;
use mm_nodes_hlsl::{convert, ShaderHlslKind, Stage, CODE_LABEL, CODE_PARAM};
use mm_types::{MmError, ParamValue, RationalTime, Size};

use crate::common::{dxc, fixture, gpu_and_dxc, params_of, render_with};

/// 3 行目の 12 列目に、宣言していない名前がある。
const UNDECLARED: &str =
    "float4 mm_main(float2 uv) {\n    float4 c = 0;\n    return c + nothere;\n}\n";

/// DXC は通るが naga の読めない命令（DebugPrintf の拡張）を 3 行目に出す。
const PRINTF: &str = "float4 mm_main(float2 uv) {\n    float4 c = float4(uv, 0, 1);\n    printf(\"%f\", c.x);\n    return c;\n}\n";

#[test]
fn dxc_の誤りは利用者の行と列で出る() {
    let Some(dxc) = dxc() else { return };
    let err = convert(&dxc, UNDECLARED, CODE_LABEL, 0).unwrap_err();
    assert_eq!(err.stage, Stage::Dxc);
    let first = &err.diagnostics[0];
    assert_eq!(first.file.as_deref(), Some(CODE_LABEL));
    assert_eq!((first.line, first.column), (Some(3), Some(16)), "{err}");
    assert!(err.to_string().contains("code:3:16:"), "{err}");
}

#[test]
fn naga_の読めない_spirv_は命令の行で出る() {
    let Some(dxc) = dxc() else { return };
    let err = convert(&dxc, PRINTF, CODE_LABEL, 0).unwrap_err();
    eprintln!("{err}");
    assert_eq!(err.stage, Stage::Naga, "{err}");
    assert_eq!(err.diagnostics[0].line, Some(3), "{err}");
    assert!(err.to_string().contains("code:3:"), "{err}");
}

#[test]
fn 変換の誤りはそのレイヤーの_invalid_argument_になる() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let kind = ShaderHlslKind::with_dxc(Some(dxc));
    let params = params_of(&kind, &[(CODE_PARAM, ParamValue::Text(UNDECLARED.into()))]);
    let mut wgpu = mm_gpu::WgpuGpu::new(ctx);
    let at = (Size::new(8, 8), RationalTime::ZERO);
    let err = render_with(&kind, &params, &mut wgpu, Vec::new(), at).unwrap_err();
    assert!(
        matches!(err, MmError::InvalidArgument(ref m) if m.contains("code:3:16:")),
        "{err:?}"
    );
    assert_eq!(kind.id(), "shader.hlsl");
}

#[test]
fn 同じ源は_2_回目から変換し直さない() {
    let Some(dxc) = dxc() else { return };
    // 試験ごとに源を変え、ほかの試験のキャッシュに当たらないようにする。
    let code = format!("{}// {}\n", fixture("time.hlsl"), std::process::id());
    let started = Instant::now();
    let first = convert(&dxc, &code, CODE_LABEL, 0).expect("変換できる");
    let cold = started.elapsed();
    let started = Instant::now();
    let second = convert(&dxc, &code, CODE_LABEL, 0).expect("変換できる");
    let warm = started.elapsed();
    eprintln!("HLSL → WGSL: 初回 {cold:?}、キャッシュ {warm:?}");
    assert!(std::sync::Arc::ptr_eq(&first, &second), "同じ結果を返す");
    assert!(
        warm * 10 < cold,
        "キャッシュは DXC を起こさない（{cold:?} と {warm:?}）"
    );
    assert!(first.contains("fn mm_fs"), "入口が残る:\n{first}");
}

#[test]
fn 入力の数が違えば別の束ね方で変換する() {
    let Some(dxc) = dxc() else { return };
    let code = fixture("mix.hlsl");
    let one = convert(&dxc, &code, CODE_LABEL, 1).expect("変換できる");
    let two = convert(&dxc, &code, CODE_LABEL, 2).expect("変換できる");
    assert!(
        one.contains("@binding(2)") && !one.contains("@binding(3)"),
        "1 つならサンプラーは 2:\n{one}"
    );
    assert!(
        two.contains("@binding(3)"),
        "2 つならサンプラーは 3:\n{two}"
    );
}
