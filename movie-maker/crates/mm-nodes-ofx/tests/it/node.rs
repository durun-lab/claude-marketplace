//! 種類として描くテスト（`mm_testkit::node::Harness`、CPU の画像）。契約、値の受け渡し、パラメータの一覧、置き場所の決まり、期待画像。
//!
//! 期待画像（`fixtures/golden/cpu/ofx-gain.png`）の更新:
//! `MM_GOLDEN_UPDATE=1 MM_GOLDEN_BACKEND=cpu cargo test -p mm-nodes-ofx --test it node::`

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_host_ofx::TEST_PLUGINS;
use mm_node_api::{InputFrames, NodeKind};
use mm_nodes_ofx::{OfxKind, ASSET_PARAM, BUNDLE_PARAM, PLUGIN_PARAM};
use mm_testkit::golden::{Golden, GOLDEN_SIZE};
use mm_testkit::node::{contract, ConstInputs, Harness, NodeFixture};
use mm_types::{AssetId, MmError, ParamValue, Params, RationalTime, Result, Rgba, RgbaImage, Size};

use crate::support::{host, kind, params, GAIN, GAIN_ID, INVERT};

/// 期待画像のフォルダー名。プラグインは CPU で描き、GPU のバックエンドに依らない。
const BACKEND: &str = "cpu";
/// 8 ビットの量子化の誤差。
const BYTE_TOL: f32 = 1.0 / 255.0 + 1e-6;

/// 左上が赤、右下が半透明の青、ほかは灰色。
fn source(size: Size) -> RgbaImage {
    let mut img = RgbaImage::filled(size, Rgba::new(0.25, 0.25, 0.25, 1.0));
    img.set_pixel(0, 0, Rgba::new(1.0, 0.0, 0.0, 1.0));
    img.set_pixel(
        size.width - 1,
        size.height - 1,
        Rgba::new(0.0, 0.0, 0.5, 0.5),
    );
    img
}

/// 横に赤、縦に緑が増え、青は一定のグラデーション。
fn gradient(size: Size) -> RgbaImage {
    let mut img = RgbaImage::transparent(size);
    for y in 0..size.height {
        for x in 0..size.width {
            let r = x as f32 / (size.width - 1) as f32;
            let g = y as f32 / (size.height - 1) as f32;
            img.set_pixel(x, y, Rgba::new(r, g, 0.4, 1.0));
        }
    }
    img
}

fn render(kind: &OfxKind, p: &Params, input: &RgbaImage) -> Result<Arc<RgbaImage>> {
    let mut inputs = ConstInputs(vec![Arc::new(input.clone())]);
    render_with(kind, p, input.size(), &mut inputs)
}

fn render_with(
    kind: &OfxKind,
    p: &Params,
    size: Size,
    inputs: &mut dyn InputFrames,
) -> Result<Arc<RgbaImage>> {
    let mut h = Harness::new(None);
    h.render(kind, p, RationalTime::seconds(1), size, 1.0, inputs)
        .map(|(image, _)| image)
}

#[test]
fn テストのプラグインで契約を満たす() {
    let Some(exe) = host() else { return };
    // 真だとレンダラーがバンドルをデコーダーで開こうとして、そのレイヤーが描けない。
    assert!(!kind(&exe).decodes_asset());
    // プラグインを指さない既定値は入力をそのまま返す（恒等）ので、どの場合も入力を渡す。
    for p in [
        params(GAIN, GAIN_ID, &[]),
        params(INVERT, "", &[]),
        Params::new(),
    ] {
        let fx = NodeFixture {
            params: p,
            inputs: vec![Arc::new(source(Size::new(64, 36)))],
            ..NodeFixture::default()
        };
        contract(|| Arc::new(kind(&exe)), &fx);
    }
}

#[test]
fn 時刻で評価した値を渡して描く() {
    let Some(exe) = host() else { return };
    let k = kind(&exe);
    let src = source(Size::new(4, 3));
    let same = render(&k, &params(GAIN, GAIN_ID, &[]), &src).unwrap();
    assert_eq!(*same, src, "既定の gain 1 はそのまま");
    let values = [
        ("gain", ParamValue::Number(0.5)),
        ("channel", ParamValue::Choice("red".into())),
        ("stale", ParamValue::Number(1.0)),
    ];
    let out = render(&k, &params(GAIN, GAIN_ID, &values), &src).expect("知らない名前は渡さない");
    assert_eq!(out.pixel(0, 0), Rgba::new(0.5, 0.0, 0.0, 1.0));
    assert_eq!(out.pixel(1, 1), Rgba::new(0.125, 0.25, 0.25, 1.0));
}

#[test]
fn 公式の_invert_を描ける() {
    let Some(exe) = host() else { return };
    let src = source(Size::new(4, 3));
    let out = render(&kind(&exe), &params(INVERT, "", &[]), &src).unwrap();
    let want =
        RgbaImage::from_raw(src.size(), src.data().iter().map(|v| 1.0 - v).collect()).unwrap();
    assert!(out.max_diff(&want).unwrap() <= BYTE_TOL);
}

#[test]
fn 選んだ効果のパラメータを一覧に出す() {
    let Some(exe) = host() else { return };
    let k = kind(&exe);
    let names = |p: &Params| -> Vec<String> {
        k.params_for(p).params.into_iter().map(|s| s.name).collect()
    };
    let own = [BUNDLE_PARAM, ASSET_PARAM, PLUGIN_PARAM];
    assert_eq!(names(&Params::new()), own, "何も指さなければ自前の分だけ");
    let gain = names(&params(GAIN, "", &[]));
    assert_eq!(gain[..3], own);
    assert_eq!(
        gain[3..],
        ["gain", "channel", "delay_ms"],
        "隠しのものは出さない"
    );
    let schema = k.params_for(&params(GAIN, "", &[]));
    assert_eq!(
        schema.get("channel").map(|s| s.default.clone()),
        Some(ParamValue::Choice("all".into()))
    );
}

#[test]
fn 置き場所の外のバンドルは描かない() {
    let Some(exe) = host() else { return };
    let k = kind(&exe);
    let src = source(Size::new(4, 3));
    let outside = std::env::temp_dir().join(format!("mm-nodes-ofx-out-{}", std::process::id()));
    let fake = outside.join(GAIN);
    std::fs::create_dir_all(&fake).unwrap();
    let cases = [
        fake.to_string_lossy().into_owned(),
        format!("../{GAIN}"),
        "C:/Windows/System32".into(),
    ];
    for bundle in cases {
        let err = render(&k, &params(&bundle, "", &[]), &src);
        assert!(
            matches!(err, Err(MmError::InvalidArgument(_))),
            "{bundle}: {err:?}"
        );
    }
    std::fs::remove_dir_all(&outside).unwrap();
}

#[test]
fn プロジェクトの素材のバンドルで描ける() {
    let Some(exe) = host() else { return };
    let src = source(Size::new(4, 3));
    // ConstInputs は素材の ID をそのままパスとして引く。
    let bundle: PathBuf = Path::new(TEST_PLUGINS).join(GAIN);
    let mut p = params("", "", &[("gain", ParamValue::Number(0.0))]);
    p.set(
        ASSET_PARAM,
        ParamValue::AssetRef(AssetId::new(bundle.to_string_lossy())),
    );
    let out = render(&kind(&exe), &p, &src).expect("素材のバンドル");
    assert_eq!(out.pixel(1, 1), Rgba::new(0.0, 0.0, 0.0, 1.0));
}

#[test]
fn プラグインを指さなければ入力をそのまま返す() {
    let Some(exe) = host() else { return };
    let src = source(Size::new(4, 3));
    let out = render(&kind(&exe), &Params::new(), &src).unwrap();
    assert_eq!(*out, src);
}

#[test]
fn gain_の期待画像() {
    let Some(exe) = host() else { return };
    let src = gradient(GOLDEN_SIZE);
    let values = [
        ("gain", ParamValue::Number(0.5)),
        ("channel", ParamValue::Choice("red".into())),
    ];
    let out = render(&kind(&exe), &params(GAIN, GAIN_ID, &values), &src).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden");
    let actual = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden-actual");
    let Some(golden) = Golden::open(&root, BACKEND, &actual) else {
        return;
    };
    if let Err(message) = golden.check("ofx-gain", &out) {
        panic!("{message}");
    }
}
