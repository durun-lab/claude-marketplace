//! `transition.wgsl` をシーケンスの編集点で描く。レンダラーが進み具合と長さを渡すこと、`rewind` で出ていくカットを
//! 逆向きに描くことを、画素の値（線形の光）で確かめる。GPU が無い環編集点ではスキップする。

use crate::common;

use common::{comp, gpu, layer, project, render_at, renderer, seconds, shader};
use mm_doc::{Cut, Interpolation, Keyframe, Node, Project, Sequence, Transition};
use mm_types::{Id, ParamValue, RationalTime, Size};

const SMALL: Size = Size::new(64, 36);
/// f16 の丸めを見込んだ許容。
const TOLERANCE: f32 = 3e-3;

/// 時刻を色にするカット。赤 = 素材の秒 / 4。
const CLOCK: &str = "fn mm_main(uv: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(u.time / 4.0, u.p0.x, 0.0, 1.0);\n}\n";

fn transition(code: &str, rewind: f64) -> Transition {
    Transition {
        node: Node::new("transition.wgsl", 1)
            .with_param("code", ParamValue::Text(code.into()))
            .with_param("rewind", ParamValue::Number(rewind)),
        duration: seconds(1),
    }
}

/// 2 秒のカット a（緑 0）と b（緑 1）。b の先頭に `tr`。掛かるのは 1.5..2.5 秒。
fn two_cuts(tr: Transition) -> Project {
    let cut = |id: &str, green: f64| {
        let node = shader(CLOCK).with_param("p0", ParamValue::Number(green));
        Cut::new(id, node, seconds(0), seconds(2))
    };
    let mut b = cut("b", 1.0);
    b.transition_in = Some(tr);
    let seq = Sequence {
        id: Id::new("s"),
        cuts: vec![cut("a", 0.0), b],
        origin: None,
    };
    let node = Node::new("sequence", 1).with_param("sequence", ParamValue::Text("s".into()));
    project(
        vec![comp("main", SMALL, 4, vec![layer("seq", node, 4)])],
        vec![seq],
    )
}

fn at(num: i128, den: u32) -> RationalTime {
    RationalTime::new(num, den).unwrap()
}

fn near(got: f32, want: f32, what: &str) {
    assert!(
        (got - want).abs() <= TOLERANCE,
        "{what}: {want} のはずが {got}"
    );
}

#[test]
fn 進み具合と長さと縦横比を受ける() {
    let Some(ctx) = gpu() else { return };
    let code = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(u.progress, u.duration / 4.0, u.ratio / 4.0, u.time);\n}\n";
    let mut r = renderer(&ctx);
    let img = render_at(
        &mut r,
        &two_cuts(transition(code, 0.0)),
        "main",
        at(7, 4),
        SMALL,
    );
    let px = img.pixel(10, 10);
    near(px.r, 0.25, "progress");
    near(px.g, 0.25, "duration");
    near(px.b, 64.0 / 36.0 / 4.0, "ratio");
    near(px.a, 0.25, "time");
    assert!(r.take_errors().is_empty());
}

#[test]
fn rewind_が正なら出ていくカットを掛かり始めから逆向きに描く() {
    let Some(ctx) = gpu() else { return };
    let from = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {\n    return mm_from(uv);\n}\n";
    let mut r = renderer(&ctx);
    // 1.75 秒は掛かり始め（1.5 秒）の 0.25 秒後。普通なら素材の 1.75 秒、3 倍の逆向きなら 1.5 - 0.75 = 0.75 秒。
    let plain = render_at(
        &mut r,
        &two_cuts(transition(from, 0.0)),
        "main",
        at(7, 4),
        SMALL,
    );
    near(plain.pixel(10, 10).r, 1.75 / 4.0, "普通の向き");
    let back = render_at(
        &mut r,
        &two_cuts(transition(from, 3.0)),
        "main",
        at(7, 4),
        SMALL,
    );
    near(back.pixel(10, 10).r, 0.75 / 4.0, "逆向き");
    // カットの先頭より前には戻らない。
    let floor = render_at(
        &mut r,
        &two_cuts(transition(from, 9.0)),
        "main",
        at(9, 4),
        SMALL,
    );
    near(floor.pixel(10, 10).r, 0.0, "カットの先頭で止まる");
    assert!(r.take_errors().is_empty());
}

/// 2 秒のカットを並べた sequence を 1 枚のレイヤーで描く 6 秒のコンポジション。
fn cuts_project(cuts: Vec<Cut>) -> Project {
    let seq = Sequence {
        id: Id::new("s"),
        cuts,
        origin: None,
    };
    let node = Node::new("sequence", 1).with_param("sequence", ParamValue::Text("s".into()));
    project(
        vec![comp("main", SMALL, 6, vec![layer("seq", node, 6)])],
        vec![seq],
    )
}

fn clock_cut(id: &str, tr: Option<Transition>) -> Cut {
    let mut c = Cut::new(id, shader(CLOCK), seconds(0), seconds(2));
    c.transition_in = tr;
    c
}

fn with_duration(mut tr: Transition, secs: i64) -> Transition {
    tr.duration = seconds(secs);
    tr
}

const FROM: &str = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {\n    return mm_from(uv);\n}\n";

#[test]
fn 逆向きの倍率はキーフレームでも編集点の値で読む() {
    let Some(ctx) = gpu() else { return };
    let mut tr = transition(FROM, 0.0);
    // 編集点（2 秒）で倍率 2。時刻 0 の値（0）を読むと普通の向きになる。
    tr.node.params.get_mut("rewind").unwrap().keyframes = vec![
        Keyframe::new(seconds(0), ParamValue::Number(0.0), Interpolation::Linear),
        Keyframe::new(seconds(4), ParamValue::Number(4.0), Interpolation::Linear),
    ];
    let mut r = renderer(&ctx);
    let img = render_at(&mut r, &two_cuts(tr), "main", at(7, 4), SMALL);
    // 掛かり始め 1.5 秒の 0.25 秒後、2 倍の逆向きなら素材の 1.5 - 0.5 = 1.0 秒。
    near(img.pixel(10, 10).r, 1.0 / 4.0, "編集点の倍率で逆向き");
    assert!(r.take_errors().is_empty());
}

#[test]
fn トランジションが両側のカットより長くてもカットの長さで頭打ちになる() {
    let Some(ctx) = gpu() else { return };
    let code = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(u.progress, u.duration / 4.0, 0.0, 1.0);\n}\n";
    let tr = with_duration(transition(code, 0.0), 10);
    let p = cuts_project(vec![clock_cut("a", None), clock_cut("b", Some(tr))]);
    let mut r = renderer(&ctx);
    let img = render_at(&mut r, &p, "main", at(1, 1), SMALL);
    // 半分は両側のカットの長さ 2 秒、全体 4 秒で、1 秒は掛かり始め（0 秒）の 1/4。
    near(img.pixel(10, 10).r, 0.25, "progress");
    near(img.pixel(10, 10).g, 1.0, "掛かる長さ 4 秒");
    assert!(r.take_errors().is_empty());
}

#[test]
fn 先頭のカットに置いたトランジションは描かない() {
    let Some(ctx) = gpu() else { return };
    let tr = transition(FROM, 3.0);
    let p = cuts_project(vec![clock_cut("a", Some(tr))]);
    let mut r = renderer(&ctx);
    let img = render_at(&mut r, &p, "main", at(1, 1), SMALL);
    near(img.pixel(10, 10).r, 1.0 / 4.0, "普通の向きのカット");
    assert!(r.take_errors().is_empty());
}

#[test]
fn 両端にトランジションのあるカットは長さを分け合い逆向きでもカットの先頭で止まる() {
    let Some(ctx) = gpu() else { return };
    // b は 2 秒で、両端に長さ 4 秒（半分 2 秒）のトランジション。分け合うので半分は 1 秒になる。
    let p = cuts_project(vec![
        clock_cut("a", None),
        clock_cut("b", Some(with_duration(transition(FROM, 9.0), 4))),
        clock_cut("c", Some(with_duration(transition(FROM, 0.0), 4))),
    ]);
    let mut r = renderer(&ctx);
    // a と b の編集点（2 秒）は 1..3 秒。1.5 秒は掛かり始めの 0.5 秒後で、9 倍に戻ると a の先頭で止まる。
    let first = render_at(&mut r, &p, "main", at(3, 2), SMALL);
    near(first.pixel(10, 10).r, 0.0, "a の先頭で止まる");
    // b と c の編集点（4 秒）は 3..5 秒。3.25 秒の出ていく b は、c にトランジションが無いときと同じ普通の向き。
    let second = render_at(&mut r, &p, "main", at(13, 4), SMALL);
    let plain = cuts_project(vec![clock_cut("a", None), clock_cut("b", None)]);
    let want = render_at(&mut r, &plain, "main", at(13, 4), SMALL)
        .pixel(10, 10)
        .r;
    near(second.pixel(10, 10).r, want, "b は普通の向き");
    assert!(r.take_errors().is_empty());
}

#[test]
fn コンパイルに失敗したらエラーを記録する() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let broken = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> { return nope; }";
    render_at(
        &mut r,
        &two_cuts(transition(broken, 0.0)),
        "main",
        at(2, 1),
        SMALL,
    );
    let errors = r.take_errors();
    assert!(
        errors.iter().any(|e| e.contains("WGSL")),
        "コンパイルの失敗が残る: {errors:?}"
    );
}
