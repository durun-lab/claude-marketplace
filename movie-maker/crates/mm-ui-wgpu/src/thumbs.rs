//! 帯の先頭のサムネイルを描く。サムネイルは 1 枚のテクスチャ（アトラス）のスロットに並べ、見えている帯の全部を
//! 1 回の描画呼び出しで描く（数千のカットでも帯ごとに呼び出さない）。
//! スロットが埋まったら、直近のフレームで見えていなかったサムネイルのうち、いちばん前に使ったものから捨てる。
//! サムネイルの読み込みはアプリの側が別のスレッドで行い、`Ui::set_thumb` でスロットに書き込む。読み終わるまではサムネイルなしで描く。

use std::collections::HashMap;
use std::sync::Arc;

use mm_ui_layout::{Rect, ThumbQuad};

use gpu::{Gpu, Instance};
use slot_image::level_range;
pub use slot_image::{slot_image_from_rgb, slot_image_len};

mod gpu;
mod slot_image;

/// アトラスの 1 スロットの大きさ（物理ピクセル）。`THUMB_ASPECT` と同じ 16:9。Large の帯（94x53 論理ピクセル）を
/// 表示の倍率 2 で描いても引き伸ばさない大きさで、キャッシュの JPEG（幅 320）より小さい。
pub const SLOT_W: u32 = 192;
pub const SLOT_H: u32 = 108;
pub(super) const COLS: u32 = 10;
pub(super) const ROWS: u32 = 20;
/// アトラスに入るサムネイルの数。1 フレームに見える数（行の数と、シーケンスの行のカットの数で 100 に満たない）より多くし、
/// 見えなくなったサムネイルもしばらく残して、スクロールで戻ったときに読み直さない。
const CAPACITY: u32 = COLS * ROWS;
/// アトラスのミップレベルの数。Medium の帯を倍率 1 で描くとスロットを 4 分の 1 ほどに縮めるので、
/// 縦横を 2 回半分にしたレベルまで持ち、1 画素に入る画素を間引かずに平均する（スロットの大きさは 4 で割り切れる）。
pub(super) const MIPS: u32 = 3;
/// スロットの縁から内側に取る幅（ミップレベル 0 の画素）。いちばん縮めたレベルの半画素で、隣のスロットの色を混ぜない。
const EDGE: f32 = (1u32 << (MIPS - 1)) as f32 / 2.0;
/// 入りきらなかったサムネイルを、次に頼むまでに待つフレームの数（毎フレーム読み直さない）。
const DEFER_FRAMES: u64 = 60;

/// サムネイル 1 枚の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    /// スロットに入っている。`used` は最後に描いたフレーム。
    Ready { slot: u32, used: u64 },
    /// 読み込みを頼んで、結果を待っている。
    Pending,
    /// 読めなかった（ファイルが無い、壊れている）。取り込みが素材の出力を作り終えるまで頼み直さない。
    Missing,
    /// 読めなかったものを読み直している。読めるまでは読めなかったものとして扱う（帯から外したままにする）。
    Retrying,
    /// スロットが空かず入れられなかった。`retry` のフレームから頼み直す。
    Deferred { retry: u64 },
}

/// サムネイルのキーとスロットの割り当て。GPU に触れないので、割り当てと捨て方を単体で試せる。
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

    /// 次のフレームの描画を始める。最後に描いたフレームはこの番号で記録する。
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// サムネイル `key` を描くなら、そのスロット。入っていれば最後に描いたフレームを記録し、無ければ読み込みを頼む
    /// （頼み済みなら頼まない）。
    pub(crate) fn slot_to_draw(&mut self, key: &Arc<str>) -> Option<u32> {
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

    /// 読めなかったサムネイルとして記録する。同じキーを 2 回読んで前の方が読めていたら、スロットのものを残す。
    pub(crate) fn missing(&mut self, key: &str) {
        if !matches!(self.entries.get(key), Some(Entry::Ready { .. })) {
            self.entries.insert(key.into(), Entry::Missing);
        }
    }

    /// 読めなかったサムネイルの読み込みを頼み直す（取り込みでサムネイルができたかもしれない）。
    /// 読み直している途中のものも頼み直し、前の読み込みがファイルのできる前だったときに取りこぼさない。
    pub(crate) fn retry_missing(&mut self) {
        for (key, entry) in &mut self.entries {
            if matches!(entry, Entry::Missing | Entry::Retrying) {
                *entry = Entry::Retrying;
                self.requests.push(key.clone());
            }
        }
    }

    /// `key` が読めなかったサムネイル（読み直している途中を含む）か。
    pub(crate) fn is_missing(&self, key: &str) -> bool {
        matches!(
            self.entries.get(key),
            Some(Entry::Missing | Entry::Retrying)
        )
    }

    /// 読めなかったサムネイルが 1 つでもあるか。
    pub(crate) fn has_missing(&self) -> bool {
        self.entries
            .values()
            .any(|e| matches!(e, Entry::Missing | Entry::Retrying))
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

    /// 読み込んだスロットの画像 `image`（`slot_image_from_rgb` の大きさ）をスロットに書く。
    /// `None` と大きさ違いは読めなかったサムネイルとして記録する。
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        image: Option<&[u8]>,
    ) {
        let Some(image) = image.filter(|i| i.len() == slot_image_len()) else {
            self.slots.missing(key);
            return;
        };
        let Some(slot) = self.slots.insert(key) else {
            return;
        };
        let format = self.format;
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(device, format));
        for mip in 0..MIPS {
            write_level(queue, &gpu.texture, slot, mip, &image[level_range(mip)]);
        }
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
                let slot = self.slots.slot_to_draw(&q.key)?;
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
fn slot_origin(slot: u32) -> (u32, u32) {
    ((slot % COLS) * SLOT_W, (slot / COLS) * SLOT_H)
}

/// スロット `slot` のミップレベル `mip` に、そのレベルの大きさの RGBA8 の画像 `rgba` を書く。
fn write_level(queue: &wgpu::Queue, texture: &wgpu::Texture, slot: u32, mip: u32, rgba: &[u8]) {
    let (x, y) = slot_origin(slot);
    let (w, h) = (SLOT_W >> mip, SLOT_H >> mip);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: wgpu::Origin3d {
                x: x >> mip,
                y: y >> mip,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}

/// スロット `slot` の中の `part`（0..1）に当たる、アトラスの範囲の左上と右下（0..1）。
/// スロットの縁を `EDGE` だけ内側に取り、どのミップレベルでも隣のスロットの色を混ぜない。
fn atlas_uv(slot: u32, part: Rect) -> [f32; 4] {
    let (ox, oy) = slot_origin(slot);
    let (w, h) = ((COLS * SLOT_W) as f32, (ROWS * SLOT_H) as f32);
    let inner = |o: u32, size: u32, t: f32| o as f32 + EDGE + t * (size as f32 - 2.0 * EDGE);
    [
        inner(ox, SLOT_W, part.x) / w,
        inner(oy, SLOT_H, part.y) / h,
        inner(ox, SLOT_W, part.right()) / w,
        inner(oy, SLOT_H, part.bottom()) / h,
    ]
}

#[cfg(test)]
mod tests;
