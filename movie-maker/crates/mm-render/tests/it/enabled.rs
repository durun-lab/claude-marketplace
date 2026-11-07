//! 無効にしたレイヤー（`Layer.enabled = false`）は合成から外れる。トラックマットには使える。
//! GPU が無い環境ではスキップする。

use crate::common;

use common::{comp, gpu, layer, project, render_at, renderer, seconds, solid};
use mm_doc::{Layer, MatteKind, Project, TrackMatte};
use mm_render::WgpuRenderer;
use mm_types::{Id, ParamValue, Rgba, RgbaImage, Size};
use std::sync::Arc;

const SIZE: Size = Size::new(64, 36);
/// 中間の画像は半精度の浮動小数なので、1e-3 程度の丸めが出る。
const TOLERANCE: f32 = 3e-3;
const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::new(0.0, 0.0, 1.0, 1.0);
const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);
const CENTER: (u32, u32) = (32, 18);

fn disabled(mut l: Layer) -> Layer {
    l.enabled = false;
    l
}

fn render(p: &Project) -> Option<(Arc<RgbaImage>, Vec<String>)> {
    let ctx = gpu()?;
    let mut r: WgpuRenderer = renderer(&ctx);
    let img = render_at(&mut r, p, "main", seconds(0), SIZE);
    Some((img, r.take_errors()))
}

fn assert_center(img: &RgbaImage, want: Rgba, what: &str) {
    let got = img.pixel(CENTER.0, CENTER.1);
    assert!(
        got.max_diff(want) <= TOLERANCE,
        "{what}: {want:?} のはずが {got:?}"
    );
}

fn matted_by(mut l: Layer, matte: &str) -> Layer {
    l.track_matte = Some(TrackMatte {
        layer: Id::new(matte),
        kind: MatteKind::Alpha,
        inverted: false,
    });
    l
}

#[test]
fn 無効のレイヤーは描かず_下のレイヤーが見える() {
    let layers = vec![
        layer("under", solid(BLUE), 2),
        disabled(layer("top", solid(RED), 2)),
    ];
    let Some((img, errors)) = render(&project(vec![comp("main", SIZE, 2, layers)], vec![])) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_center(&img, BLUE, "無効の赤は描かない");
}

#[test]
fn 無効にした切り抜くレイヤーは描かない() {
    let layers = vec![
        disabled(matted_by(layer("target", solid(RED), 2), "matte")),
        layer("matte", solid(WHITE), 2),
    ];
    let Some((img, _)) = render(&project(vec![comp("main", SIZE, 2, layers)], vec![])) else {
        return;
    };
    assert_center(
        &img,
        Rgba::TRANSPARENT,
        "無効の切り抜くレイヤーもマットも描かない",
    );
}

#[test]
fn 無効にしたマットのレイヤーもマットとして効き_描かない() {
    let layers = vec![
        matted_by(layer("target", solid(RED), 2), "matte"),
        disabled(layer("matte", solid(WHITE), 2)),
    ];
    let Some((img, errors)) = render(&project(vec![comp("main", SIZE, 2, layers)], vec![])) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_center(&img, RED, "無効のマットでも切り抜く（白は描かない）");
}

#[test]
fn 入れ子の中の無効のレイヤーも描かない() {
    let inner = comp(
        "inner",
        SIZE,
        2,
        vec![
            layer("under", solid(BLUE), 2),
            disabled(layer("top", solid(RED), 2)),
        ],
    );
    let node = mm_doc::Node::new("composition", 1)
        .with_param("composition", ParamValue::Text("inner".into()));
    let p = project(
        vec![inner, comp("main", SIZE, 2, vec![layer("n", node, 2)])],
        vec![],
    );
    let Some((img, _)) = render(&p) else { return };
    assert_center(&img, BLUE, "入れ子の中の無効の赤は描かない");
}
