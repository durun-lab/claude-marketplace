//! 自前のカーソル（roll、slip、slide、刃）の絵。0..1 の座標の多角形を、白の塗りと黒の縁で 4x4 のスーパーサンプリングで描く。
//! 表示の倍率ごとの大きさ（32、40、48 など）で作り直すので、どの倍率でもぼやけない。
//! 手本は Premiere と Resolve のカーソル（roll は編集点の両側へ向く矢印、slip は枠の中身が動く矢印、slide は枠ごと動く矢印、
//! 刃は縦の刃物）で、画像のファイルを持たずコードで描く。

use mm_ui_layout::CursorKind;

/// 1 画素の縦横のサンプル数。
const SS: usize = 4;
/// 縁の太さ（カーソルの大きさ 32 のときの画素数。大きさに比例して太くする）。
const OUTLINE_PX: f32 = 1.0;

/// 多角形（0..1 の座標）。
type Poly = Vec<(f32, f32)>;

/// 描いた絵。`bgra` は乗算済みアルファの 32 ビット、行は上から。
#[derive(Debug, Clone, PartialEq)]
pub struct Art {
    pub size: u32,
    pub bgra: Vec<u8>,
    /// ホットスポット（画素）。
    pub hot: (u32, u32),
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Poly {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// 左向きの矢印。`tip` が先、`base` が三角の底の x、`stem` が軸の終わりの x（右）。
fn arrow_left(tip: f32, base: f32, stem: f32) -> Vec<Poly> {
    let head = vec![(tip, 0.5), (base, 0.5 - 0.17), (base, 0.5 + 0.17)];
    vec![head, rect(base - 0.01, 0.47, stem, 0.53)]
}

/// 右向きの矢印（`arrow_left` を x = 0.5 で折り返したもの。引数は左向きのときの値）。
fn arrow_right(tip: f32, base: f32, stem: f32) -> Vec<Poly> {
    mirror(arrow_left(tip, base, stem))
}

fn mirror(polys: Vec<Poly>) -> Vec<Poly> {
    polys
        .into_iter()
        .map(|p| p.into_iter().map(|(x, y)| (1.0 - x, y)).collect())
        .collect()
}

/// 形ごとの多角形。
fn shapes(kind: CursorKind) -> Vec<Poly> {
    let mut out = Vec::new();
    match kind {
        // 編集点の両側へ向く矢印（縦の 2 本線が編集点）。
        CursorKind::Roll => {
            out.push(rect(0.44, 0.2, 0.47, 0.8));
            out.push(rect(0.53, 0.2, 0.56, 0.8));
            out.extend(arrow_left(0.08, 0.28, 0.44));
            out.extend(arrow_right(0.08, 0.28, 0.44));
        }
        // 枠（カットの窓）の外へ向く矢印。窓は中身が動くので枠だけにする。
        CursorKind::Slip => {
            out.push(rect(0.36, 0.30, 0.64, 0.34));
            out.push(rect(0.36, 0.66, 0.64, 0.70));
            out.push(rect(0.36, 0.30, 0.40, 0.70));
            out.push(rect(0.60, 0.30, 0.64, 0.70));
            out.extend(arrow_left(0.08, 0.26, 0.36));
            out.extend(arrow_right(0.08, 0.26, 0.36));
        }
        // 塗りつぶしの枠（カットごと動く）と、前後のカットとの境の縦線、外へ向く矢印。
        CursorKind::Slide => {
            out.push(rect(0.40, 0.35, 0.60, 0.65));
            out.push(rect(0.30, 0.25, 0.33, 0.75));
            out.push(rect(0.67, 0.25, 0.70, 0.75));
            out.extend(arrow_left(0.06, 0.20, 0.30));
            out.extend(arrow_right(0.06, 0.20, 0.30));
        }
        // 縦の刃物（持ち手と、下へ尖る刃）。割る時刻は中央の縦線。
        CursorKind::Blade => {
            out.push(rect(0.44, 0.04, 0.56, 0.24));
            out.push(vec![
                (0.36, 0.26),
                (0.64, 0.26),
                (0.64, 0.62),
                (0.5, 0.96),
                (0.36, 0.62),
            ]);
        }
        CursorKind::Arrow | CursorKind::SizeWE | CursorKind::SizeNS | CursorKind::No => {}
    }
    out
}

/// 点が多角形の中か（偶奇規則）。
fn inside(poly: &Poly, (px, py): (f32, f32)) -> bool {
    let mut hit = false;
    let n = poly.len();
    for i in 0..n {
        let ((x0, y0), (x1, y1)) = (poly[i], poly[(i + 1) % n]);
        if (y0 > py) != (y1 > py) && px < (x1 - x0) * (py - y0) / (y1 - y0) + x0 {
            hit = !hit;
        }
    }
    hit
}

/// 一辺 `n * SS` のサンプルの、塗りの範囲。
fn fill_mask(polys: &[Poly], n: usize) -> Vec<bool> {
    let side = n * SS;
    let mut mask = vec![false; side * side];
    for (i, m) in mask.iter_mut().enumerate() {
        let p = (
            ((i % side) as f32 + 0.5) / side as f32,
            ((i / side) as f32 + 0.5) / side as f32,
        );
        *m = polys.iter().any(|poly| inside(poly, p));
    }
    mask
}

/// `mask` を半径 `r`（サンプル）だけ広げた範囲。
fn dilate(mask: &[bool], side: usize, r: f32) -> Vec<bool> {
    let reach = r.ceil() as i32;
    let offsets: Vec<(i32, i32)> = (-reach..=reach)
        .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| ((dx * dx + dy * dy) as f32) <= r * r)
        .collect();
    let at = |x: i32, y: i32| {
        (0..side as i32).contains(&x)
            && (0..side as i32).contains(&y)
            && mask[y as usize * side + x as usize]
    };
    (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as i32, (i / side) as i32);
            offsets.iter().any(|(dx, dy)| at(x + dx, y + dy))
        })
        .collect()
}

/// 形 `kind` を一辺 `size` 画素で描く。矢印と禁止は標準のカーソルなので描かない（空の絵）。
pub fn render(kind: CursorKind, size: u32) -> Art {
    let n = size as usize;
    let polys = shapes(kind);
    let fill = fill_mask(&polys, n);
    let side = n * SS;
    let r = OUTLINE_PX * size as f32 / 32.0 * SS as f32;
    let edge = dilate(&fill, side, r);
    let mut bgra = vec![0u8; n * n * 4];
    let samples = (SS * SS) as f32;
    for py in 0..n {
        for px in 0..n {
            let (mut f, mut e) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let i = (py * SS + sy) * side + px * SS + sx;
                    f += u32::from(fill[i]);
                    e += u32::from(edge[i]);
                }
            }
            let (f, e) = (f as f32 / samples, e as f32 / samples);
            let o = (py * n + px) * 4;
            // 乗算済みアルファ。白の塗りは明るさ f、縁の黒は 0 で、alpha は縁を含めた被り。
            let v = (f * 255.0).round() as u8;
            bgra[o..o + 4].copy_from_slice(&[v, v, v, (e * 255.0).round() as u8]);
        }
    }
    Art {
        size,
        bgra,
        hot: (size / 2, size / 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [CursorKind; 4] = [
        CursorKind::Roll,
        CursorKind::Slip,
        CursorKind::Slide,
        CursorKind::Blade,
    ];

    fn alpha(a: &Art, x: u32, y: u32) -> u8 {
        a.bgra[((y * a.size + x) * 4 + 3) as usize]
    }

    fn opaque(a: &Art) -> usize {
        a.bgra.chunks(4).filter(|p| p[3] == 255).count()
    }

    #[test]
    fn 表示の倍率_100_125_150_の大きさで描き_形の外の角は透明で_ホットスポットは絵の中() {
        for size in [32, 40, 48] {
            for kind in KINDS {
                let a = render(kind, size);
                assert_eq!(a.bgra.len(), (size * size * 4) as usize, "{kind:?} {size}");
                assert!(
                    opaque(&a) > (size * size / 20) as usize,
                    "{kind:?} {size} を描いた"
                );
                assert_eq!(alpha(&a, 0, 0), 0, "{kind:?} {size} の左上は透明");
                assert!(a.hot.0 < size && a.hot.1 < size);
            }
        }
    }

    #[test]
    fn 形は互いに違い_大きさが変わっても同じ形() {
        let shapes: Vec<Art> = KINDS.iter().map(|k| render(*k, 32)).collect();
        for i in 0..shapes.len() {
            for j in i + 1..shapes.len() {
                assert_ne!(
                    shapes[i].bgra, shapes[j].bgra,
                    "{:?} と {:?}",
                    KINDS[i], KINDS[j]
                );
            }
        }
        // 大きさ 64 を 32 に縮めた不透明の割合は、32 で描いた割合と近い。
        let big = render(CursorKind::Roll, 64);
        let small = render(CursorKind::Roll, 32);
        let ratio = |a: &Art| opaque(a) as f32 / (a.size * a.size) as f32;
        assert!(
            (ratio(&big) - ratio(&small)).abs() < 0.03,
            "{} {}",
            ratio(&big),
            ratio(&small)
        );
    }

    #[test]
    fn 縁は白の塗りの外に黒で付き_塗りは白() {
        let a = render(CursorKind::Slide, 32);
        // 中央の塗りは白（乗算済みで rgb = alpha = 255）。
        let c = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(&a.bgra[c..c + 4], &[255, 255, 255, 255]);
        // 縁の画素は、不透明で rgb が 0（黒）。
        let has_black = a.bgra.chunks(4).any(|p| p[3] == 255 && p[0] == 0);
        assert!(has_black, "黒の縁");
    }

    #[test]
    fn 標準のカーソルの形は描かない() {
        for kind in [
            CursorKind::Arrow,
            CursorKind::SizeWE,
            CursorKind::SizeNS,
            CursorKind::No,
        ] {
            assert_eq!(opaque(&render(kind, 32)), 0);
        }
    }
}
