//! 通し: 置く → 見積もり → 承認 → 生成 → 素材になる → 描く → 取り消し → 使い回し。

use std::path::Path;

use mm_doc::Project;
use mm_node_api::NoInputs;
use mm_nodes_genai::{
    asset_id_for, media_dir, FakeProvider, GenAiKind, GenRequest, Reuse, ASSET_PARAM,
};
use mm_ops::{Entry, OpContext};
use mm_testkit::node::Harness;
use mm_types::{AssetId, LayerId, ParamValue, Params, RationalTime, Size};

use crate::common::{editor_with_layer, fake_color, genai, genai_node, PngSource, HEIGHT, WIDTH};

const FAKE_IMAGE_USD: f64 = 0.04;

/// レイヤーの `asset`。置く前は無い（既定値の空）。
fn placed(project: &Project, layer: &LayerId) -> Option<AssetId> {
    let (_, l) = project.layer(layer).unwrap();
    l.source
        .params
        .get(ASSET_PARAM)
        .and_then(|p| p.value.as_asset().cloned())
}

fn asset_of(project: &Project, layer: &LayerId) -> AssetId {
    placed(project, layer).expect("素材が置いてある")
}

fn params_of(project: &Project, layer: &LayerId) -> Params {
    let (_, l) = project.layer(layer).unwrap();
    let mut p = Params::new();
    for (k, v) in &l.source.params {
        p.set(k.clone(), v.value.clone());
    }
    p
}

/// 素材のファイルを読んで種類に描かせる。
fn render(project: &Project, layer: &LayerId) -> mm_types::RgbaImage {
    let asset = project.asset(&asset_of(project, layer)).unwrap();
    let source = PngSource::open(Path::new(&asset.path)).unwrap();
    let mut h = Harness::new(Some(Box::new(source)));
    let size = Size::new(WIDTH as u32, HEIGHT as u32);
    let (img, rod) = h
        .render(
            &GenAiKind,
            &params_of(project, layer),
            RationalTime::ZERO,
            size,
            1.0,
            &mut NoInputs,
        )
        .unwrap();
    assert_eq!(rod, size.to_rect(), "同じ縦横比なら画布の全体");
    (*img).clone()
}

#[test]
fn 置いて見積もって承認して生成した素材を描き_取り消せる() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (mut ed, layer) = editor_with_layer(genai_node(&[]));

    let quote = gen.quote(ed.project(), &layer).unwrap();
    assert!((quote.estimate().cost_usd - FAKE_IMAGE_USD).abs() < 1e-12);
    assert_eq!(quote.reuse(), &Reuse::None);
    assert_eq!(fake.calls(), 0, "見積もりでは提供元を呼ばない");

    let approval = gen.approve(&quote, Entry::Surface).unwrap();
    let made = gen.generate(ed.project(), &approval).unwrap();
    assert!(made.called_provider);
    assert_eq!(fake.calls(), 1);
    assert_eq!(made.asset, asset_id_for(quote.key()));

    ed.apply(made.transaction.unwrap(), &OpContext::human())
        .unwrap();
    let asset = ed.project().asset(&made.asset).unwrap().clone();
    assert!(Path::new(&asset.path).starts_with(std::path::absolute(dir.path()).unwrap()));
    assert_eq!(asset_of(ed.project(), &layer), made.asset);

    let req = GenRequest::from_params(
        &params_of(ed.project(), &layer).with_defaults(&mm_node_api::NodeKind::params(&GenAiKind)),
    )
    .unwrap();
    let img = render(ed.project(), &layer);
    assert_eq!(img.pixel(3, 3), fake_color(FakeProvider::tint(&req)));

    ed.undo(&OpContext::human()).unwrap().expect("取り消せる");
    assert!(ed.project().asset(&made.asset).is_err(), "素材が外れる");
    assert_eq!(
        placed(ed.project(), &layer),
        None,
        "パラメータが置く前に戻る"
    );
    assert!(Path::new(&asset.path).exists(), "ファイルは残る");
}

#[test]
fn 取り消した後の同じ頼みは提供元を呼ばずに置き直す() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (mut ed, layer) = editor_with_layer(genai_node(&[]));
    let first = gen.quote(ed.project(), &layer).unwrap();
    let made = gen
        .generate(ed.project(), &gen.approve(&first, Entry::Surface).unwrap())
        .unwrap();
    ed.apply(made.transaction.unwrap(), &OpContext::human())
        .unwrap();
    ed.undo(&OpContext::human()).unwrap();

    let again = gen.quote(ed.project(), &layer).unwrap();
    assert!(matches!(again.reuse(), Reuse::File { .. }), "{again:?}");
    assert_eq!(again.estimate().cost_usd, 0.0);
    let remade = gen
        .generate(ed.project(), &gen.approve(&again, Entry::Surface).unwrap())
        .unwrap();
    assert!(!remade.called_provider);
    ed.apply(remade.transaction.unwrap(), &OpContext::human())
        .unwrap();
    assert_eq!(fake.calls(), 1, "使い回しでは呼ばない");
    assert_eq!(asset_of(ed.project(), &layer), made.asset);
}

#[test]
fn 同じ頼みの別のレイヤーはプロジェクトの素材を向け直すだけ() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (mut ed, layer) = editor_with_layer(genai_node(&[]));
    let q = gen.quote(ed.project(), &layer).unwrap();
    let made = gen
        .generate(ed.project(), &gen.approve(&q, Entry::Surface).unwrap())
        .unwrap();
    ed.apply(made.transaction.unwrap(), &OpContext::human())
        .unwrap();

    let q = gen.quote(ed.project(), &layer).unwrap();
    assert_eq!(q.reuse(), &Reuse::Asset);
    let none = gen
        .generate(ed.project(), &gen.approve(&q, Entry::Surface).unwrap())
        .unwrap();
    assert!(none.transaction.is_none(), "もう向いている");
    assert_eq!(fake.calls(), 1);
}

#[test]
fn パラメータを変えると別の素材になる() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (ed, layer) = editor_with_layer(genai_node(&[]));
    let (ed2, _) = editor_with_layer(genai_node(&[("seed", ParamValue::Number(7.0))]));
    let a = gen.quote(ed.project(), &layer).unwrap();
    let b = gen.quote(ed2.project(), &layer).unwrap();
    assert_ne!(a.key(), b.key());
    gen.generate(ed.project(), &gen.approve(&a, Entry::Surface).unwrap())
        .unwrap();
    gen.generate(ed2.project(), &gen.approve(&b, Entry::Surface).unwrap())
        .unwrap();
    assert_eq!(fake.calls(), 2);
}
