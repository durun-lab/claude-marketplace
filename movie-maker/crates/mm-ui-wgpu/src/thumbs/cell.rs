//! スロットに入れる画像（RGBA8）を CPU で作る。キャッシュの JPEG を 16:9 に切り出して縮め、縮小の段（ミップマップ）も作る。
//! アプリが読み込みのスレッドで作るので、描画スレッドは書くだけで済む。

use std::ops::Range;

use super::{CELL_H, CELL_W, MIPS};

/// アトラスに入れるサムネイルのバイト数。`cell_from_rgb` が作る、スロットの大きさの RGBA8 と、
/// それを縦横半分にしていった縮小の段を、段の順に並べたもの。
pub fn cell_len() -> usize {
    level_range(MIPS - 1).end
}

/// 縮小の段 `mip` の、`cell_from_rgb` の並びの中の範囲。
pub(super) fn level_range(mip: u32) -> Range<usize> {
    let size = |m: u32| ((CELL_W >> m) * (CELL_H >> m) * 4) as usize;
    let start = (0..mip).map(size).sum();
    start..start + size(mip)
}

/// 幅 `w`、高さ `h` の RGB8 の画像を、16:9 の中央に切り出してスロットの大きさに縮めた RGBA8 にし、縮小の段を続ける。
/// 縮めるときは元の画素の範囲を平均する（そのまま間引くと細い線がちらつく）。大きさが合わなければ `None`。
pub fn cell_from_rgb(w: u32, h: u32, rgb: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || rgb.len() != (w * h * 3) as usize {
        return None;
    }
    let (w_f, h_f) = (w as f32, h as f32);
    let aspect = CELL_W as f32 / CELL_H as f32;
    let (cw, ch) = if w_f / h_f > aspect {
        (h_f * aspect, h_f)
    } else {
        (w_f, w_f / aspect)
    };
    let (x0, y0) = ((w_f - cw) / 2.0, (h_f - ch) / 2.0);
    let mut out = Vec::with_capacity(cell_len());
    for dy in 0..CELL_H {
        let rows = span(y0, ch, dy, CELL_H, h);
        for dx in 0..CELL_W {
            let cols = span(x0, cw, dx, CELL_W, w);
            out.extend(average(rgb, w, rows, cols));
            out.push(255);
        }
    }
    for mip in 1..MIPS {
        let half = halve(
            &out[level_range(mip - 1)],
            CELL_W >> (mip - 1),
            CELL_H >> (mip - 1),
        );
        out.extend(half);
    }
    Some(out)
}

/// 切り出した範囲（始まり `start`、長さ `len`）を `cells` 個に分けた `i` 番目に当たる、元の画素の範囲。1 画素は必ず含む。
fn span(start: f32, len: f32, i: u32, cells: u32, max: u32) -> (u32, u32) {
    let step = len / cells as f32;
    let a = ((start + i as f32 * step).floor() as u32).min(max - 1);
    let b = ((start + (i + 1) as f32 * step).ceil() as u32).clamp(a + 1, max);
    (a, b)
}

/// 幅 `w` の RGB8 の画像の、行 `rows` と列 `cols`（どちらも始まりと終わり）の範囲の平均の色。
fn average(rgb: &[u8], w: u32, (ys, ye): (u32, u32), (xs, xe): (u32, u32)) -> [u8; 3] {
    let mut sum = [0u32; 3];
    for y in ys..ye {
        let row = &rgb[((y * w + xs) * 3) as usize..((y * w + xe) * 3) as usize];
        for px in row.chunks_exact(3) {
            sum.iter_mut()
                .zip(px)
                .for_each(|(s, v)| *s += u32::from(*v));
        }
    }
    let n = (ye - ys) * (xe - xs);
    sum.map(|s| (s / n) as u8)
}

/// RGBA8 の `w`x`h`（どちらも偶数）の画像を、2x2 の画素の平均で縦横半分にする（縮小の次の段）。
pub(super) fn halve(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let at = |x: u32, y: u32, c: u32| u32::from(rgba[((y * w + x) * 4 + c) as usize]);
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            out.extend((0..4).map(|c| {
                let sum = at(x, y, c) + at(x + 1, y, c) + at(x, y + 1, c) + at(x + 1, y + 1, c);
                ((sum + 2) / 4) as u8
            }));
        }
    }
    out
}
