//! 期待画像のテスト。フィクスチャの Lottie を 2 つの時刻で描き、`tests/it/golden/vello_cpu/` の PNG と比べる
//! （`mm_testkit::golden`）。CPU だけで描くので GPU のバックエンドによらず 1 組。
//! 更新は `MM_GOLDEN_UPDATE=1` を付けて回し、書き出した PNG を目で確かめてからコミットする。

use std::path::Path;

use mm_nodes_lottie::LottieKind;
use mm_testkit::golden::{Golden, GOLDEN_SIZE};
use mm_types::RationalTime;

use crate::{fixture, params_for, render_at, secs};

/// 期待画像のフォルダー名。描くのは vello_cpu だけ。
const BACKEND: &str = "vello_cpu";

fn golden() -> Golden {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/golden");
    let actual = std::env::temp_dir().join("mm-nodes-lottie-golden");
    Golden::open(&root, BACKEND, &actual).expect("期待画像のフォルダーはリポジトリにある")
}

#[test]
fn 時刻ごとの絵が期待画像と合う() {
    let g = golden();
    let kind = LottieKind::default();
    let params = params_for(&fixture("shapes.json"));
    for (name, t) in [
        ("shapes_t0", RationalTime::ZERO),
        ("shapes_t1_25", secs(5, 4)),
    ] {
        let image = render_at(&kind, &params, t, GOLDEN_SIZE).expect("描ける");
        g.check(name, &image).unwrap_or_else(|e| panic!("{e}"));
    }
}
