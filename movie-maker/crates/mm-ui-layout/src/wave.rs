//! 帯の波形。キャッシュの波形（1 秒に 100 個のピーク）を、素材の範囲とズームに合わせて縦の矩形の高さにする。
//! 描くのは `paint/wave.rs`。ここは素材の時刻への換算、ズームごとのピークの取り直し、描いた結果の使い回しを持つ。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::geom::{Quad, Rect};
use crate::gesture::EditKind;
use crate::keymarks::MARK_PX;
use crate::rows::RowHeight;
use crate::time_view::TimeView;

/// 帯の波形の元。コンポジションの時刻 `at` に素材の時刻 `src`（秒）が当たり、素材は `speed` 倍の速さで進む。
#[derive(Debug, Clone, PartialEq)]
pub struct WaveSource {
    /// 素材の ID（[`Waves`] を引く）。
    pub asset: Arc<str>,
    pub at: f64,
    pub src: f64,
    pub speed: f64,
}

impl WaveSource {
    /// コンポジションの時刻 `t` の素材の時刻。
    pub fn src_at(&self, t: f64) -> f64 {
        self.src + (t - self.at) * self.speed
    }
}

/// 素材 1 つのピーク。ズームごとに粗さを変えられるよう、2 個ずつの最大を重ねた段を持つ（作るのは読み込みの 1 回だけ）。
#[derive(Debug, PartialEq)]
pub struct Peaks {
    per_second: f64,
    /// 段 `k` の要素は、元のピーク `2^k` 個の最大。段 0 は全チャンネルの最大。
    levels: Vec<Vec<f32>>,
}

impl Peaks {
    /// チャンネルごとにインターリーブしたピーク（`MediaState::waveform` と同じ、1 秒に `per_second` 個）から作る。
    pub fn new(per_second: f64, channels: usize, interleaved: &[f32]) -> Self {
        let channels = channels.max(1);
        let base: Vec<f32> = interleaved
            .chunks(channels)
            .map(|c| c.iter().fold(0.0, |a: f32, v| a.max(v.abs())))
            .collect();
        let mut levels = vec![base];
        while levels.last().is_some_and(|l| l.len() > 1) {
            let next = levels[levels.len() - 1]
                .chunks(2)
                .map(|c| c.iter().fold(0.0, |a: f32, v| a.max(*v)))
                .collect();
            levels.push(next);
        }
        Self {
            per_second: per_second.max(1e-6),
            levels,
        }
    }

    /// 素材の時刻 `[from, to)`（秒）のピークの最大。範囲が 1 個に満たないときは、含まれるピーク 1 個。範囲の外は 0。
    /// 段の境に揃えて引くので、粗い範囲では両端の外のピークを少し含む。
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
        level[lo..hi.min(level.len())]
            .iter()
            .fold(0.0, |a: f32, v| a.max(*v))
    }
}

/// 描く矩形を作り直すかを決める入力。これが同じなら、前に作った矩形を使い回す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WaveKey {
    pub time: TimeView,
    pub scroll_y: f32,
    pub area: Rect,
    pub preview: Option<(EditKind, f64)>,
}

/// 素材の ID ごとのピークと、描いた矩形の使い回し。
#[derive(Debug, Default)]
pub struct Waves {
    peaks: HashMap<Arc<str>, Arc<Peaks>>,
    memo: RefCell<Option<(WaveKey, Arc<[Quad]>)>>,
}

impl Waves {
    /// 素材のピークを置く（読み込みが終わった素材から）。
    pub fn insert(&mut self, asset: Arc<str>, peaks: Peaks) {
        self.peaks.insert(asset, Arc::new(peaks));
        self.invalidate();
    }

    /// 素材のピークがあるか。
    pub fn contains(&self, asset: &str) -> bool {
        self.peaks.contains_key(asset)
    }

    /// モデルや行の高さが変わったときに、使い回しを捨てる（`WaveKey` に入らない入力）。
    pub fn invalidate(&mut self) {
        *self.memo.get_mut() = None;
    }

    pub(crate) fn get(&self, asset: &str) -> Option<&Peaks> {
        self.peaks.get(asset).map(Arc::as_ref)
    }

    /// `key` が前と同じなら前の矩形、違えば `build` で作り直した矩形。
    pub(crate) fn quads(&self, key: WaveKey, build: impl FnOnce() -> Vec<Quad>) -> Arc<[Quad]> {
        if let Some((k, quads)) = &*self.memo.borrow() {
            if *k == key {
                return quads.clone();
            }
        }
        let quads: Arc<[Quad]> = build().into();
        *self.memo.borrow_mut() = Some((key, quads.clone()));
        quads
    }
}

/// 波形の高さの 0..=1 の見た目の振幅。小さな音（話し声）も見えるよう平方根で持ち上げる。
pub(crate) fn amplitude(peak: f32) -> f32 {
    peak.clamp(0.0, 1.0).sqrt()
}

/// 帯 `bar` の中の波形の領域。Small は出さない。Medium は下端のマーカーの高さ（マーカーはこの上に重なる）、
/// Large は帯の下半分。名前は帯の上の部分なので重ならない。
pub(crate) fn region(height: RowHeight, bar: Rect) -> Option<Rect> {
    let h = match height {
        RowHeight::Small => return None,
        RowHeight::Medium => MARK_PX,
        RowHeight::Large => (bar.h / 2.0).floor(),
    };
    (bar.h >= h && h > 0.0).then(|| Rect::new(bar.x, bar.bottom() - h, bar.w, h))
}

/// 横 `[x0, x1)`（Timeline の座標）を `step` px ごとの列にした、元 `src` の振幅（0..=1）。列は左から順。
/// 1 列が受け持つ素材の時間（拡大で短く、縮小で長く）のピークの最大を取る。
pub(crate) fn amplitudes(
    peaks: &Peaks,
    src: &WaveSource,
    t_of: impl Fn(f32) -> f64,
    (x0, x1): (f32, f32),
    step: f32,
) -> Vec<f32> {
    let n = ((x1 - x0) / step).ceil().max(0.0) as usize;
    (0..n)
        .map(|i| {
            let x = x0 + i as f32 * step;
            let (a, b) = (src.src_at(t_of(x)), src.src_at(t_of(x + step)));
            amplitude(peaks.max_in(a.min(b), a.max(b)))
        })
        .collect()
}

#[cfg(test)]
mod tests;
