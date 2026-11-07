//! 文字のスタイルの文字が、プレートの中に置かれるかの検査。`text` は画像の左上を `position` に `bounds` の位置を足して
//! 決めるので、中央揃えで `bounds` が右にずれるシェーパーでも、字の塊の中心がプレートの中心に来る必要がある。

use mm_doc::Node;
use mm_pack::expand;
use mm_testkit::text::SampleShaper;
use mm_text_api::{TextAlign, TextShaper, TextStyle};
use mm_types::ParamValue;

use crate::vlog::{apply_on_episode, args, vlog};

/// 画素の丸めで許す差。
const TOLERANCE_PX: f64 = 0.5;

fn number(node: &Node, name: &str) -> f64 {
    match node.params[name].value {
        ParamValue::Number(v) => v,
        ref other => panic!("{name} は数でない: {other:?}"),
    }
}

/// `text` の種類が描くときの字の塊の左端と幅（コンポジションの画素）。
fn ink_span(node: &Node) -> (f64, f64) {
    let ParamValue::Vec2([x, _]) = node.params["position"].value else {
        panic!("position が無い");
    };
    let ParamValue::Text(text) = &node.params["text"].value else {
        panic!("text が無い");
    };
    let style = TextStyle {
        size_px: number(node, "size") as f32,
        max_width: Some(number(node, "max_width") as f32).filter(|w| *w > 0.0),
        align: TextAlign::Center,
        ..TextStyle::default()
    };
    let bounds = SampleShaper.shape(text, &style).expect("組める").bounds;
    (x + f64::from(bounds.x), f64::from(bounds.width))
}

#[test]
fn 中央揃えの字幕の字の塊はプレートの中心に来る() {
    let pack = vlog();
    let txn = expand(
        &pack,
        "caption",
        &args(&[
            ("id", "c1"),
            ("text", "リバーブの設定を聞いてみます"),
            ("at", "0"),
        ]),
        &SampleShaper,
    )
    .expect("展開できる");
    let project = apply_on_episode(&pack, txn);
    let layers = &project.compositions[0].layers;
    let plate = &layers[0].source;
    let canvas_w = f64::from(pack.theme.canvas.width);
    let plate_center = (number(plate, "p0") + number(plate, "p2") / 2.0) * canvas_w;
    let (left, width) = ink_span(&layers[1].source);
    let ink_center = left + width / 2.0;
    assert!(
        (ink_center - plate_center).abs() <= TOLERANCE_PX,
        "字の塊の中心 {ink_center} とプレートの中心 {plate_center}"
    );
}
