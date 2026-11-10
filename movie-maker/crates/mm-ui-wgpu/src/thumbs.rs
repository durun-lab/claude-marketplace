//! 帯の先頭のサムネイルを描く。サムネイルは 1 枚のテクスチャ（アトラス）のスロットに並べ、見えている帯の全部を
//! 1 回の描画呼び出しで描く（数千のカットでも帯ごとに呼び出さない）。
//! スロットが埋まったら、直近のフレームで見えていなかったサムネイルのうち、いちばん前に使ったものから捨てる。
//! サムネイルの読み込みはアプリの側が別のスレッドで行い、`Ui::set_thumb` でスロットに書き込む。読み終わるまではサムネイルなしで描く。

use std::collections::HashMap;
use std::sync::Arc;

use mm_ui_layout::{Rect, ThumbQuad};

use gpu::{Gpu, Instance};

mod gpu;

/// アトラスの 1 スロットの大きさ（物理ピクセル）。`THUMB_ASPECT` と同じ 16:9。Large の帯（高さ 53 論理ピクセル）を
/// 表示の倍率 1.5 で描いても足りる大きさ。
pub const CELL_W: u32 = 128;
pub const CELL_H: u32 = 72;
pub(super) const COLS: u32 = 16;
pub(super) const ROWS: u32 = 28;
/// アトラスに入るサムネイルの数。
const CAPACITY: u32 = COLS * ROWS;
/// 入りきらなかったサムネイルを、次に頼むまでに待つフレームの数（毎フレーム読み直さない）。
const DEFER_FRAMES: u64 = 60;

/// アトラスに入れるサムネイル。`cell_from_rgb` が作る、スロットちょうどの大きさの RGBA8。
pub fn cell_len() -> usize {
    (CELL_W * CELL_H * 4) as usize
}

/// 幅 `w`、高さ `h` の RGB8 の画像を、16:9 の中央に切り出してスロットの大きさに縮めた RGBA8 にする。
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
        let (ys, ye) = span(y0, ch, dy, CELL_H, h);
        for dx in 0..CELL_W {
            let (xs, xe) = span(x0, cw, dx, CELL_W, w);
            let mut sum = [0u32; 3];
            for y in ys..ye {
                for x in xs..xe {
                    let i = ((y * w + x) * 3) as usize;
                    sum.iter_mut()
                        .zip(&rgb[i..i + 3])
                        .for_each(|(s, v)| *s += u32::from(*v));
                }
            }
            let n = (ye - ys) * (xe - xs);
            out.extend(sum.iter().map(|s| (s / n) as u8));
            out.push(255);
        }
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

/// サムネイル 1 枚の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    /// スロットに入っている。`used` は最後に描いたフレーム。
    Ready { slot: u32, used: u64 },
    /// 読み込みを頼んで、結果を待っている。
    Pending,
    /// 読めなかった（ファイルが無い、壊れている）。モデルが置き換わるまで頼み直さない。
    Missing,
    /// スロットが空かず入れられなかった。`retry` のフレームから頼み直す。
    Deferred { retry: u64 },
}

/// サムネイルの鍵とスロットの割り当て。GPU に触れないので、割り当てと捨て方を単体で試せる。
pub(crate) struct Slots {
    entries: HashMap<Arc<str>, Entry>,
    free: Vec<u32>,
    frame: u64,
    requests: Vec<Arc<str>>,
}

impl Slots {
    pub(crate) fn new(capacity: u32) -> Self {
        Self {
            entries: HashMap::new(),
            free: (0..capacity).rev().collect(),
            frame: 0,
            requests: Vec::new(),
        }
    }

    /// 次のフレームの描画を始める。使った印はこの番号で付ける。
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// サムネイル `key` を描くなら、そのスロット。入っていれば使った印を付け、無ければ読み込みを頼む（頼み済みなら頼まない）。
    pub(crate) fn look(&mut self, key: &Arc<str>) -> Option<u32> {
        if let Some(Entry::Ready { slot, used }) = self.entries.get_mut(&**key) {
            *used = self.frame;
            return Some(*slot);
        }
        let due = match self.entries.get(&**key) {
            None => true,
            Some(Entry::Deferred { retry }) => *retry <= self.frame,
            Some(_) => false,
        };
        if due {
            self.entries.insert(key.clone(), Entry::Pending);
            self.requests.push(key.clone());
        }
        None
    }

    /// 頼んだサムネイルのうち、まだ頼んでいないもの（頼んだ順）。
    pub(crate) fn take_requests(&mut self) -> Vec<Arc<str>> {
        std::mem::take(&mut self.requests)
    }

    /// 読み込めたサムネイルを入れるスロットを決める。空きが無ければ、直近のフレームで描かなかったサムネイルのうち
    /// いちばん前に使ったものを捨てて使う。全部が直近のフレームで見えているなら入れず、少し後に頼み直す。
    pub(crate) fn insert(&mut self, key: &str) -> Option<u32> {
        if let Some(Entry::Ready { slot, .. }) = self.entries.get(key) {
            return Some(*slot);
        }
        let Some(slot) = self.free.pop().or_else(|| self.evict()) else {
            let retry = self.frame + DEFER_FRAMES;
            self.entries.insert(key.into(), Entry::Deferred { retry });
            return None;
        };
        self.entries.insert(
            key.into(),
            Entry::Ready {
                slot,
                used: self.frame,
            },
        );
        Some(slot)
    }

    /// 見えていない（`used` が今のフレームより前の）サムネイルのうち、いちばん前に使ったものを捨て、そのスロットを返す。
    fn evict(&mut self) -> Option<u32> {
        let (key, slot) = self
            .entries
            .iter()
            .filter_map(|(k, e)| match e {
                Entry::Ready { slot, used } if *used < self.frame => Some((k, *slot, *used)),
                _ => None,
            })
            .min_by_key(|(_, _, used)| *used)
            .map(|(k, slot, _)| (k.clone(), slot))?;
        self.entries.remove(&key);
        Some(slot)
    }

    /// 読めなかったサムネイルに印を付ける。
    pub(crate) fn missing(&mut self, key: &str) {
        self.entries.insert(key.into(), Entry::Missing);
    }

    /// 読めなかった印を外す（取り込みでサムネイルができたかもしれない）。スロットに入っているものは残す。
    pub(crate) fn retry_missing(&mut self) {
        self.entries.retain(|_, e| *e != Entry::Missing);
    }

    /// `key` が読めなかったサムネイルか。
    pub(crate) fn is_missing(&self, key: &str) -> bool {
        self.entries.get(key) == Some(&Entry::Missing)
    }

    /// 読めなかったサムネイルが 1 つでもあるか。
    pub(crate) fn has_missing(&self) -> bool {
        self.entries.values().any(|e| *e == Entry::Missing)
    }

    /// スロットに入っているサムネイルの数。
    pub(crate) fn resident(&self) -> usize {
        self.entries
            .values()
            .filter(|e| matches!(e, Entry::Ready { .. }))
            .count()
    }
}

/// サムネイルの描画。
pub(crate) struct ThumbRenderer {
    format: wgpu::TextureFormat,
    slots: Slots,
    gpu: Option<Gpu>,
    /// 直近のフレームで描いた数。
    drawn: u32,
}

impl ThumbRenderer {
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            slots: Slots::new(CAPACITY),
            gpu: None,
            drawn: 0,
        }
    }

    pub(crate) fn take_requests(&mut self) -> Vec<Arc<str>> {
        self.slots.take_requests()
    }

    pub(crate) fn retry_missing(&mut self) {
        self.slots.retry_missing();
    }

    pub(crate) fn is_missing(&self, key: &str) -> bool {
        self.slots.is_missing(key)
    }

    pub(crate) fn has_missing(&self) -> bool {
        self.slots.has_missing()
    }

    pub(crate) fn resident(&self) -> usize {
        self.slots.resident()
    }

    pub(crate) fn drawn(&self) -> u32 {
        self.drawn
    }

    /// 読み込んだサムネイル `cell`（`cell_from_rgb` の大きさ）をスロットに書く。`None` と大きさ違いは読めなかったサムネイルとして覚える。
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        cell: Option<&[u8]>,
    ) {
        let Some(cell) = cell.filter(|c| c.len() == cell_len()) else {
            self.slots.missing(key);
            return;
        };
        let Some(slot) = self.slots.insert(key) else {
            return;
        };
        let format = self.format;
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(device, format));
        let (x, y) = cell_origin(slot);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            cell,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(CELL_W * 4),
                rows_per_image: Some(CELL_H),
            },
            wgpu::Extent3d {
                width: CELL_W,
                height: CELL_H,
                depth_or_array_layers: 1,
            },
        );
    }

    /// 描くサムネイル（物理ピクセルの矩形）を転送し、描く数を返す。スロットに入っていないサムネイルは読み込みを頼み、今は描かない。
    /// パスの外（`submit` の前）で 1 フレームに 1 回だけ呼ぶ。
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[ThumbQuad],
        viewport: (f32, f32),
    ) -> u32 {
        self.slots.begin_frame();
        let instances: Vec<Instance> = quads
            .iter()
            .filter_map(|q| {
                let slot = self.slots.look(&q.key)?;
                Some(Instance {
                    rect: [q.rect.x, q.rect.y, q.rect.w, q.rect.h],
                    uv: atlas_uv(slot, q.uv),
                    alpha: q.alpha,
                })
            })
            .collect();
        self.drawn = 0;
        let Some(gpu) = self.gpu.as_mut().filter(|_| !instances.is_empty()) else {
            return 0;
        };
        gpu.write(device, queue, &instances, viewport);
        self.drawn = instances.len() as u32;
        self.drawn
    }

    /// `prepare` が返した数のサムネイルを 1 回で描く。
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, count: u32) {
        let Some(gpu) = self.gpu.as_ref().filter(|_| count > 0) else {
            return;
        };
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &gpu.bind, &[]);
        pass.set_vertex_buffer(0, gpu.instances.slice(..));
        pass.draw(0..6, 0..count);
    }
}

/// スロット `slot` の左上（テクスチャの画素）。
fn cell_origin(slot: u32) -> (u32, u32) {
    ((slot % COLS) * CELL_W, (slot / COLS) * CELL_H)
}

/// スロット `slot` の中の `part`（0..1）に当たる、アトラスの範囲の左上と右下（0..1）。
/// スロットの縁の画素を半画素ずつ内側に取り、隣のスロットの色を混ぜない。
fn atlas_uv(slot: u32, part: Rect) -> [f32; 4] {
    let (ox, oy) = cell_origin(slot);
    let (w, h) = ((COLS * CELL_W) as f32, (ROWS * CELL_H) as f32);
    let inner = |o: u32, size: u32, t: f32| o as f32 + 0.5 + t * (size as f32 - 1.0);
    [
        inner(ox, CELL_W, part.x) / w,
        inner(oy, CELL_H, part.y) / h,
        inner(ox, CELL_W, part.right()) / w,
        inner(oy, CELL_H, part.bottom()) / h,
    ]
}

#[cfg(test)]
mod tests;
