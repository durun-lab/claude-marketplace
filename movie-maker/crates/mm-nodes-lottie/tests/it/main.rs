//! `mm-nodes-lottie` の結合テスト。リンクを 1 回にするため、1 つの実行ファイルにまとめる（docs/ci.md）。

mod contract;
mod golden;
mod kind;
mod render;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_node_api::NodeKind;
use mm_nodes_lottie::ANIMATION_PARAM;
use mm_testkit::node::{ConstInputs, Harness};
use mm_types::{AssetId, ParamValue, Params, RationalTime, Result, RgbaImage, Size};

/// フィクスチャの Lottie のパス。
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/it/fixtures")
        .join(name)
}

/// 素材の ID にファイルのパスを入れたパラメータ。`ConstInputs` は ID をそのままパスとして答える。
pub fn params_for(path: &Path) -> Params {
    let mut p = Params::new();
    p.set(
        ANIMATION_PARAM,
        ParamValue::AssetRef(AssetId::new(path.to_string_lossy())),
    );
    p
}

/// `kind` を時刻 `t` 秒で描く（プロジェクトのデータの無いハーネス）。
pub fn render_at(
    kind: &dyn NodeKind,
    params: &Params,
    seconds: RationalTime,
    size: Size,
) -> Result<Arc<RgbaImage>> {
    let params = params.with_defaults(&kind.params());
    let mut h = Harness::new(None);
    let (image, _) = h.render(
        kind,
        &params,
        seconds,
        size,
        1.0,
        &mut ConstInputs(Vec::new()),
    )?;
    Ok(image)
}

/// 分数の秒。
pub fn secs(num: i128, den: u32) -> RationalTime {
    RationalTime::new(num, den).expect("表せる")
}
