//! 期待画像。`fixtures/golden/<バックエンド>/shader-hlsl.png` と比べる（`mm_testkit::golden`）。
//! 更新は `MM_GOLDEN_UPDATE=1` を付けてこの試験を回し、PNG を目で確かめてからコミットする。

use std::path::Path;

use mm_nodes_hlsl::{ShaderHlslKind, CODE_PARAM};
use mm_testkit::golden::{backend_name, Golden, GOLDEN_SIZE};
use mm_types::{ParamValue, RationalTime};

use crate::common::{fixture, gpu_and_dxc, gradient, params_of, render_image};

#[test]
fn 二つの入力と時刻を使う絵が期待画像と一致する() {
    let Some((ctx, dxc)) = gpu_and_dxc() else {
        return;
    };
    let backend = backend_name(&format!("{:?}", ctx.backend()), ctx.adapter_name());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden");
    let actual = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden-actual");
    let Some(golden) = Golden::open(&root, &backend, &actual) else {
        return;
    };
    let kind = ShaderHlslKind::with_dxc(Some(dxc));
    let params = params_of(
        &kind,
        &[
            (CODE_PARAM, ParamValue::Text(fixture("card.hlsl"))),
            ("p0", ParamValue::Number(0.9)),
        ],
    );
    let inputs = vec![gradient(GOLDEN_SIZE, false), gradient(GOLDEN_SIZE, true)];
    let img = render_image(
        &ctx,
        &kind,
        &params,
        inputs,
        (GOLDEN_SIZE, RationalTime::seconds(1)),
    );
    golden
        .check("shader-hlsl", &img)
        .unwrap_or_else(|e| panic!("{e}"));
}
