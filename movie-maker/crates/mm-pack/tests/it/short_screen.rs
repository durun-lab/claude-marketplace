//! 本編（1920x1080）の画素で書いたパックの部品を、大きさの違うショート（1080x1920）に置いたときの位置。
//! 文字のスタイルは基準点に近い縁からの距離を保ち（CSS の `right` や `bottom` と同じ）、web の部品には置く
//! コンポジションの画面の大きさを渡す。

use mm_doc::{Layer, Node, Project};
use mm_ops::{Editor, OpContext};
use mm_pack::{expand_for, EstimateShaper, Pack};
use mm_types::ParamValue;

use crate::vlog::{args, expand_est, vlog};

/// 画素の丸めで許す差。
const TOLERANCE_PX: f64 = 0.5;
/// ショートの画面（`theme.json` の `short`）。
const SHORT: [f64; 2] = [1080.0, 1920.0];

fn episode(pack: &Pack) -> Project {
    let mut editor = Editor::new(Project::new());
    let txn = expand_est(pack, "episode", &args(&[("n", "4")])).expect("展開できる");
    editor
        .apply(txn, &OpContext::human())
        .expect("テンプレートを適用できる");
    editor.project().clone()
}

/// エピソードのプロジェクトのショートにマクロを適用し、ショートのレイヤーを返す。
fn on_short(pack: &Pack, name: &str, pairs: &[(&str, &str)]) -> Vec<Layer> {
    let project = episode(pack);
    let mut pairs = pairs.to_vec();
    pairs.push(("comp", "short"));
    let txn = expand_for(pack, name, &args(&pairs), &EstimateShaper, &project).expect("展開できる");
    let mut editor = Editor::new(project);
    editor.apply(txn, &OpContext::human()).expect("適用できる");
    let short = editor
        .project()
        .compositions
        .iter()
        .find(|c| c.id.as_str() == "short")
        .expect("ショートがある")
        .clone();
    short.layers
}

fn number(node: &Node, name: &str) -> f64 {
    node.params[name].value.as_number().expect("数")
}

/// プレートの左上と大きさ（ショートの画素）。
fn plate_rect(node: &Node) -> [f64; 4] {
    let [w, h] = SHORT;
    [
        number(node, "p0") * w,
        number(node, "p1") * h,
        number(node, "p2") * w,
        number(node, "p3") * h,
    ]
}

#[test]
fn 右上のラベルはショートでも右と上の縁から同じ距離に置く() {
    let pack = vlog();
    let style = &pack.styles["label"];
    let margin_right = f64::from(pack.theme.canvas.width) - style.position[0];
    let layers = on_short(
        &pack,
        "label",
        &[("id", "l1"), ("text", "処理前"), ("at", "0")],
    );
    let [x, y, w, _] = plate_rect(&layers[0].source);
    assert!(
        (SHORT[0] - (x + w) - margin_right).abs() <= TOLERANCE_PX,
        "プレートの右端 {} は画面の右から {margin_right}",
        x + w
    );
    assert!((y - style.position[1]).abs() <= TOLERANCE_PX, "上端 {y}");
}

#[test]
fn 字幕はショートでも下の中央に置き_画面の幅に収める() {
    let pack = vlog();
    let style = &pack.styles["caption"];
    let margin_bottom = f64::from(pack.theme.canvas.height) - style.position[1];
    let long = "長い字幕を横に並べると画面の幅を越えるのでショートでは折り返して収めたい";
    let layers = on_short(
        &pack,
        "caption",
        &[("id", "c1"), ("text", long), ("at", "0")],
    );
    let [x, y, w, h] = plate_rect(&layers[0].source);
    assert!(
        (x + w / 2.0 - SHORT[0] / 2.0).abs() <= TOLERANCE_PX,
        "プレートの中心 {}",
        x + w / 2.0
    );
    assert!(
        (SHORT[1] - (y + h) - margin_bottom).abs() <= TOLERANCE_PX,
        "プレートの下端 {}",
        y + h
    );
    assert!(
        x >= 0.0 && x + w <= SHORT[0],
        "画面に収まる: {x}..{}",
        x + w
    );
}

#[test]
fn web_の部品にはショートの画面の大きさを渡す() {
    let pack = vlog();
    let layers = on_short(
        &pack,
        "claude",
        &[
            ("id", "cl1"),
            ("text", "ここを直す"),
            ("x", "1000"),
            ("y", "600"),
            ("at", "0"),
        ],
    );
    let ParamValue::Json(data) = &layers[0].source.params["data"].value else {
        panic!("data が JSON でない");
    };
    let canvas = &data["theme"]["canvas"];
    assert_eq!(
        (canvas["width"].as_f64(), canvas["height"].as_f64()),
        (Some(SHORT[0]), Some(SHORT[1])),
        "side=right の部品は画面の幅から右の距離を決める"
    );
}
