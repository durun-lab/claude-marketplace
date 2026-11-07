//! ノードの契約（`mm_testkit::node::contract`）。

use std::sync::Arc;

use mm_nodes_lottie::LottieKind;
use mm_testkit::node::{contract, NodeFixture};
use mm_types::{RgbaImage, Size};

use crate::{fixture, params_for};

#[test]
fn 素材の無い既定値で契約を満たす() {
    contract(|| Arc::new(LottieKind::default()), &NodeFixture::default());
}

#[test]
fn アニメーションを描いて契約を満たす() {
    let fx = NodeFixture::default();
    contract(
        || Arc::new(LottieKind::default()),
        &NodeFixture {
            params: params_for(&fixture("shapes.json")),
            // 入力は見ないが、入力があると契約のハーネスが素材のパスを答える `ConstInputs` を渡す。
            inputs: vec![Arc::new(RgbaImage::transparent(Size::new(1, 1)))],
            ..fx
        },
    );
}
