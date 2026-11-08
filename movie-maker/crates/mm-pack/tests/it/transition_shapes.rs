//! トランジションの形の検査。次の場面を見せる所（マット）を、次の場面だけ色を替えた 2 回の描画の差で取り出して測る。
//! 重ねる絵と映像の動きは場面の色に依らないので、差が出るのはマットが次の場面を見せた所だけ。

use mm_pack::Pack;
use mm_types::Size;

use crate::golden::{gpu, vlog_dir};
use crate::transitions::{base_project_with, transition_on, Stage};

/// 前の場面と、次の場面の 2 色。
const SCENE_A: &str =
    "fn mm_main(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.1, 0.1, 0.1, 1.0); }";
const SCENE_X: &str =
    "fn mm_main(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }";
const SCENE_Y: &str =
    "fn mm_main(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }";
/// 本編とショートの画面。
const MAIN: Size = Size::new(1920, 1080);
const SHORT: Size = Size::new(1080, 1920);
/// マットが次の場面を見せたとみなす差（線形の光、0..1）。
const SHOWN: f32 = 0.5;
/// liquid を 2 つの大きさで描いた画像の、画素ごとの差の平均の上限。
const LIQUID_ALIASING: f32 = 0.00125;

/// 画面 `canvas` のコンポジションに `name` を置き、進み具合 `progress` を大きさ `size` で描いたときの、画素ごとの
/// マットの見積もり（次の場面の 2 色の差の明るさ）。
fn matte(
    stage: &mut Stage,
    pack: &Pack,
    name: &str,
    canvas: Size,
    progress: f64,
    size: Size,
) -> Vec<f32> {
    let [x, y] = [SCENE_X, SCENE_Y].map(|b| {
        let project = transition_on(pack, base_project_with(canvas, SCENE_A, b), name);
        stage.render(&project, progress, size)
    });
    (0..size.height)
        .flat_map(|j| (0..size.width).map(move |i| (i, j)))
        .map(|(i, j)| (x.pixel(i, j).g - y.pixel(i, j).g).abs())
        .collect()
}

/// 行か列 `line` で、マットが見せた画素の最初と最後の位置。
fn extent(mask: &[f32], size: Size, line: Line) -> (u32, u32) {
    let shown: Vec<u32> = match line {
        Line::Row(j) => (0..size.width)
            .filter(|&i| mask[(j * size.width + i) as usize] > SHOWN)
            .collect(),
        Line::Col(i) => (0..size.height)
            .filter(|&j| mask[(j * size.width + i) as usize] > SHOWN)
            .collect(),
    };
    (
        *shown.first().expect("見せた画素がある"),
        *shown.last().unwrap(),
    )
}

#[derive(Clone, Copy)]
enum Line {
    Row(u32),
    Col(u32),
}

#[test]
fn ショートのトランジションは円を円のまま画面の中央に描く() {
    let Some(ctx) = gpu() else { return };
    let pack = Pack::load(&vlog_dir()).expect("packs/vlog を読める");
    let mut stage = Stage::new(ctx);
    let size = Size::new(216, 384);
    // レコードのラベル（中央の丸）が開き始めた所
    let mask = matte(&mut stage, &pack, "vinyl", SHORT, 0.05, size);
    let (left, right) = extent(&mask, size, Line::Row(size.height / 2));
    let (top, bottom) = extent(&mask, size, Line::Col(size.width / 2));
    let (rx, ry) = ((right - left) as i64, (bottom - top) as i64);
    assert!((rx - ry).abs() <= 2, "横の直径 {rx} と縦の直径 {ry}");
    assert!(
        ((left + right) as i64 - size.width as i64).abs() <= 2,
        "横の中心 {}",
        (left + right) / 2
    );
    assert!(
        ((top + bottom) as i64 - size.height as i64).abs() <= 2,
        "縦の中心 {}",
        (top + bottom) / 2
    );
}

#[test]
fn 大きさ_0_の形は縁のにじみで次の場面を漏らさない() {
    let Some(ctx) = gpu() else { return };
    let pack = Pack::load(&vlog_dir()).expect("packs/vlog を読める");
    let mut stage = Stage::new(ctx);
    let size = Size::new(384, 216);
    // 星座の線を引いている所。星から広がる円の半径はまだ 0
    let mask = matte(&mut stage, &pack, "stars", MAIN, 0.3, size);
    let leak = mask.iter().copied().fold(0.0_f32, f32::max);
    assert!(leak < 1e-4, "次の場面が {leak} だけ見えている");
}

/// 2 倍の大きさで描いた画像を 2x2 の平均で縮めた画像との、画素ごとの差の平均（線形の光の緑）。縁のアンチエイリアスが
/// 画素の格子に依らない見積もりなら、縁の位置と太さが解像度で変わらず、差は小さい。
fn aliasing(stage: &mut Stage, project: &mm_doc::Project, progress: f64, size: Size) -> f32 {
    let small = stage.render(project, progress, size);
    let big = stage.render(
        project,
        progress,
        Size::new(size.width * 2, size.height * 2),
    );
    let mut total = 0.0;
    for j in 0..size.height {
        for i in 0..size.width {
            let quad: f32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(di, dj)| big.pixel(2 * i + di, 2 * j + dj).g)
                .sum();
            total += (small.pixel(i, j).g - quad / 4.0).abs();
        }
    }
    total / (size.width * size.height) as f32
}

#[test]
fn liquid_の縁は描く大きさに依らず同じ所に同じ太さで出る() {
    let Some(ctx) = gpu() else { return };
    let pack = Pack::load(&vlog_dir()).expect("packs/vlog を読める");
    let mut stage = Stage::new(ctx);
    let project = transition_on(&pack, base_project_with(MAIN, SCENE_A, SCENE_X), "liquid");
    let err = aliasing(&mut stage, &project, 0.5, Size::new(480, 270));
    // シャッターの間を 8 回で描くと 0.00142、16 回で 0.00109（RTX 5090）
    assert!(err < LIQUID_ALIASING, "縁の帯が描く大きさで変わる: {err}");
}

#[test]
fn 本編のトランジションの円も円のまま() {
    let Some(ctx) = gpu() else { return };
    let pack = Pack::load(&vlog_dir()).expect("packs/vlog を読める");
    let mut stage = Stage::new(ctx);
    let size = Size::new(384, 216);
    let mask = matte(&mut stage, &pack, "vinyl", MAIN, 0.05, size);
    let (left, right) = extent(&mask, size, Line::Row(size.height / 2));
    let (top, bottom) = extent(&mask, size, Line::Col(size.width / 2));
    let (rx, ry) = ((right - left) as i64, (bottom - top) as i64);
    assert!((rx - ry).abs() <= 2, "横の直径 {rx} と縦の直径 {ry}");
}
