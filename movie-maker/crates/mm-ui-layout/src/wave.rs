//! 帯の波形。キャッシュの波形（1 秒に 100 個のピーク）を、素材の範囲とズームに合わせて縦の矩形の高さにする。
//! 描くのは `paint/wave.rs`。ここは波形の時刻への換算、ズームごとのピークの取り直し、列の並び、描いた結果の使い回しを持つ。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::geom::{Quad, Rect};
use crate::gesture::EditKind;
use crate::keymarks::MARK_PX;
use crate::rows::RowHeight;
use crate::time_view::TimeView;

/// 見た目の振幅の 1（フルスケール）に当たる 1 バイトの値。帯の波形は高さ 26 論理ピクセルまでなので、255 段で足りる。
const FULL_SCALE: f32 = 255.0;

/// 帯の波形の元。コンポジションの時刻 `comp_time` に波形の時刻 `wave_time`（秒）が当たり、波形は `speed` 倍の速さで進む。
/// 波形の時刻は音の最初のサンプルから数える（素材の時刻に、音と映像の先頭のずれを足したもの）。
#[derive(Debug, Clone, PartialEq)]
pub struct WaveSource {
    /// 素材のコンテンツハッシュ（[`Waves`] を引く）。
    pub content_hash: Arc<str>,
    pub comp_time: f64,
    pub wave_time: f64,
    pub speed: f64,
}

impl WaveSource {
    /// コンポジションの時刻 `t` の波形の時刻。
    pub fn wave_time_at(&self, t: f64) -> f64 {
        self.wave_time + (t - self.comp_time) * self.speed
    }
}

/// 素材 1 つのピーク。見た目の振幅（ピークの平方根。話し声の小さな音も見えるよう持ち上げる）を 1 バイトに丸め、
/// ズームごとに粗さを変えられるよう、2 個ずつの最大を重ねた段を持つ（作るのは読み込みの 1 回だけ）。
#[derive(Debug, PartialEq)]
pub struct Peaks {
    per_second: f64,
    /// 段 `k` の要素は、元のピーク `2^k` 個の最大。段 0 は全チャンネルの最大。
    levels: Vec<Vec<u8>>,
}

impl Peaks {
    /// チャンネルごとにインターリーブしたピーク（キャッシュの波形と同じ、1 秒に `per_second` 個）から作る。
    pub fn new(per_second: f64, channels: usize, interleaved: &[f32]) -> Self {
        let channels = channels.max(1);
        let base: Vec<u8> = interleaved
            .chunks(channels)
            .map(|c| display_byte(c.iter().fold(0.0, |a: f32, v| a.max(v.abs()))))
            .collect();
        let mut levels = vec![base];
        while levels.last().is_some_and(|l| l.len() > 1) {
            let next = levels[levels.len() - 1]
                .chunks(2)
                .map(|c| c.iter().copied().max().unwrap_or(0))
                .collect();
            levels.push(next);
        }
        Self {
            per_second: per_second.max(1e-6),
            levels,
        }
    }

    /// 波形の時刻 `[from, to)`（秒）の見た目の振幅（0..=1）の最大。範囲が 1 個に満たないときは、含まれるピーク 1 個。
    /// 範囲の外は 0。段の境に揃えて引くので、粗い範囲では両端の外のピークを少し含む。
    pub fn max_in(&self, from: f64, to: f64) -> f32 {
        let len = self.levels[0].len();
        let first = (from * self.per_second).floor().max(0.0) as usize;
        if first >= len || to < 0.0 {
            return 0.0;
        }
        let last = ((to * self.per_second).ceil() as usize).clamp(first + 1, len);
        let k = ((last - first).ilog2() as usize).min(self.levels.len() - 1);
        let level = &self.levels[k];
        let (lo, hi) = (first >> k, (last + (1 << k) - 1) >> k);
        let max = level[lo..hi.min(level.len())]
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        f32::from(max) / FULL_SCALE
    }
}

/// ピーク（0..=1）の見た目の振幅を 1 バイトにする。
fn display_byte(peak: f32) -> u8 {
    (peak.clamp(0.0, 1.0).sqrt() * FULL_SCALE).round() as u8
}

/// 波形の使い回しの鍵。描く矩形を作り直すかを決める入力で、これが同じなら前に作った矩形を使い回す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReuseKey {
    pub time: TimeView,
    pub scroll_y: f32,
    pub area: Rect,
    pub preview: Option<(EditKind, f64)>,
}

/// 素材のコンテンツハッシュごとのピークと、描いた矩形の使い回し。
#[derive(Debug, Default)]
pub struct Waves {
    peaks: HashMap<Arc<str>, Peaks>,
    /// 前に作った矩形と、そのときの使い回しの鍵。
    last_built: RefCell<Option<(ReuseKey, Arc<[Quad]>)>>,
}

impl Waves {
    /// 素材のピークを置く（読み込みが終わった素材から）。
    pub fn insert(&mut self, content_hash: Arc<str>, peaks: Peaks) {
        self.peaks.insert(content_hash, peaks);
        self.invalidate();
    }

    /// モデルや行の高さが変わったときに、使い回しを捨てる（使い回しの鍵に入らない入力）。
    pub fn invalidate(&mut self) {
        *self.last_built.get_mut() = None;
    }

    pub(crate) fn get(&self, content_hash: &str) -> Option<&Peaks> {
        self.peaks.get(content_hash)
    }

    /// `key` が前と同じなら前の矩形、違えば `build` で作り直した矩形。
    pub(crate) fn reuse_or_build(
        &self,
        key: ReuseKey,
        build: impl FnOnce() -> Vec<Quad>,
    ) -> Arc<[Quad]> {
        if let Some((k, quads)) = &*self.last_built.borrow() {
            if *k == key {
                return quads.clone();
            }
        }
        let quads: Arc<[Quad]> = build().into();
        *self.last_built.borrow_mut() = Some((key, quads.clone()));
        quads
    }
}

/// 帯 `bar` の中の波形の領域。Small は出さない。Medium は下端のマーカーの高さ（マーカーはこの上に重なる）、
/// Large は帯の下半分。名前は帯の上の部分なので重ならない。
pub(crate) fn wave_area(height: RowHeight, bar: Rect) -> Option<Rect> {
    let h = match height {
        RowHeight::Small => return None,
        RowHeight::Medium => MARK_PX,
        RowHeight::Large => (bar.h / 2.0).floor(),
    };
    (bar.h >= h && h > 0.0).then(|| Rect::new(bar.x, bar.bottom() - h, bar.w, h))
}

/// 波形の列の並び。列の境は時刻に固定する（時刻 0 から `step` px ごと）ので、横に動かしても同じ列が同じ時間を受け持ち、
/// 縮小した波形がちらつかない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Columns {
    /// 最初の列の番号（時刻 0 から始まる列が 0）。
    first: i64,
    /// 列の数。
    pub n: usize,
    /// 列の幅（論理ピクセル）。
    step: f32,
    /// 最初の列の左端（Timeline の座標）。
    left: f32,
    px_per_sec: f64,
}

impl Columns {
    /// 横 `[x0, x1)`（Timeline の座標）を覆う列。`row_x` は行の領域の左端（時刻 `time.origin` の x）。
    pub(crate) fn covering(time: &TimeView, row_x: f32, (x0, x1): (f32, f32), step: f32) -> Self {
        // 時刻 0 の x。f32 では長いコンポジションの後ろで桁が落ちるので、f64 で求める。
        let zero = f64::from(row_x) - time.origin * time.px_per_sec;
        let first = ((f64::from(x0) - zero) / f64::from(step)).floor();
        let left = (zero + first * f64::from(step)) as f32;
        Self {
            first: first as i64,
            n: ((x1 - left) / step).ceil().max(0.0) as usize,
            step,
            left,
            px_per_sec: time.px_per_sec,
        }
    }

    /// 列 `i` の左端（Timeline の座標）。
    pub(crate) fn x(&self, i: usize) -> f32 {
        self.left + i as f32 * self.step
    }

    /// 列 `i` の左端の時刻（秒）。
    fn time(&self, i: usize) -> f64 {
        (self.first + i as i64) as f64 * f64::from(self.step) / self.px_per_sec
    }
}

/// 列ごとの、波形の元 `source` の見た目の振幅（0..=1）。列は左から順。
/// 1 列が受け持つ波形の時間（拡大で短く、縮小で長く）のピークの最大を取る。
pub(crate) fn amplitudes(peaks: &Peaks, source: &WaveSource, columns: &Columns) -> Vec<f32> {
    let at = |i| source.wave_time_at(columns.time(i));
    (0..columns.n)
        .map(|i| peaks.max_in(at(i), at(i + 1)))
        .collect()
}

#[cfg(test)]
mod tests;
