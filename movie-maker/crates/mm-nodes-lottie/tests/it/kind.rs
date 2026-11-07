//! 種類の振る舞い: 時刻からフレームへの写像、決定性、覚え、置き方、壊れた素材。

use std::sync::Arc;

use mm_nodes_lottie::LottieKind;
use mm_types::{MmError, ParamValue, Params, RationalTime, RgbaImage, Size};

use crate::{fixture, params_for, render_at, secs};

const SIZE: Size = Size::new(64, 64);

fn shapes() -> Params {
    params_for(&fixture("shapes.json"))
}

fn with(mut params: Params, name: &str, value: ParamValue) -> Params {
    params.set(name, value);
    params
}

fn draw(params: &Params, t: RationalTime) -> Arc<RgbaImage> {
    render_at(&LottieKind::default(), params, t, SIZE).expect("描ける")
}

#[test]
fn 同じ時刻は描く順や覚えによらず同じ画素() {
    let kind = LottieKind::default();
    let p = shapes();
    let first = render_at(&kind, &p, secs(1, 3), SIZE).unwrap();
    render_at(&kind, &p, secs(7, 5), SIZE).unwrap();
    let again = render_at(&kind, &p, secs(1, 3), SIZE).unwrap();
    let fresh = draw(&p, secs(1, 3));
    assert_eq!(*first, *again);
    assert_eq!(*first, *fresh, "覚えの無い別の実体でも同じ");
    assert!(
        Arc::ptr_eq(&first, &again),
        "同じ入力は描き直さず覚えを返す"
    );
}

#[test]
fn 時刻が進むと絵が変わる() {
    let p = shapes();
    assert_ne!(*draw(&p, RationalTime::ZERO), *draw(&p, secs(1, 2)));
}

#[test]
fn ループは長さ_2_秒で先頭に戻る() {
    let p = shapes();
    assert_eq!(
        *draw(&p, RationalTime::ZERO),
        *draw(&p, RationalTime::seconds(2))
    );
}

#[test]
fn ループしなければ最後のフレームで止まる() {
    let p = with(shapes(), "loop", ParamValue::Bool(false));
    assert_eq!(*draw(&p, RationalTime::seconds(5)), *draw(&p, secs(59, 30)));
    assert_ne!(
        *draw(&p, RationalTime::seconds(5)),
        *draw(&p, RationalTime::ZERO)
    );
}

#[test]
fn 開始のフレームと速さで写像が変わる() {
    let base = draw(&shapes(), RationalTime::seconds(1));
    let started = with(shapes(), "start_frame", ParamValue::Number(30.0));
    assert_eq!(
        *draw(&started, RationalTime::ZERO),
        *base,
        "30 フレーム先から"
    );
    let fast = with(shapes(), "speed", ParamValue::Number(2.0));
    assert_eq!(*draw(&fast, secs(1, 2)), *base, "2 倍の速さ");
}

#[test]
fn 縦横比を保って中央に収め_none_は左上に原寸で置く() {
    let wide = Size::new(160, 80);
    let kind = LottieKind::default();
    let contain = render_at(&kind, &shapes(), RationalTime::ZERO, wide).unwrap();
    assert_eq!(contain.size(), wide);
    assert_eq!(contain.pixel(10, 40).a, 0.0, "左の余白は透明");
    assert!(contain.pixel(80, 40).a > 0.0, "中央は描かれる");
    let none = with(shapes(), "fit", ParamValue::Choice("none".into()));
    let placed = render_at(&kind, &none, RationalTime::ZERO, wide).unwrap();
    assert!(placed.pixel(10, 40).a > 0.0, "原寸の 100x100 は左上から");
    assert_eq!(placed.pixel(130, 40).a, 0.0, "100 画素より右は透明");
}

#[test]
fn 壊れた_json_と無いファイルは誤りを返す() {
    let broken = render_at(
        &LottieKind::default(),
        &params_for(&fixture("broken.json")),
        RationalTime::ZERO,
        SIZE,
    );
    assert!(
        matches!(broken, Err(MmError::InvalidArgument(_))),
        "{broken:?}"
    );
    let missing = render_at(
        &LottieKind::default(),
        &params_for(&fixture("missing.json")),
        RationalTime::ZERO,
        SIZE,
    );
    assert!(missing.is_err());
}
