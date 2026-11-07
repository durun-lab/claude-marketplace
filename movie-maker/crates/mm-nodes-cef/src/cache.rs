//! 描いたフレームのキャッシュ。ページ、パラメータ、時刻、大きさ、倍率の組で引く。
//! 静止のページは時刻を 0 に揃えるので、パラメータの組ごとに 1 枚だけ描けば済む。

use std::sync::{Arc, Weak};

use mm_gpu::GpuContext;
use mm_types::{RationalTime, RgbaImage, Size};

use crate::page::FrameRequest;

/// キャッシュするフレームの大きさの合計の上限。1080p の `Rgba16Float` は 1 枚約 16 MiB なので、1080p なら 24 枚、
/// 4K でも 6 枚持つ。1 つの画面に並ぶ静止のレイヤー（吹き出しやステッカーなど数枚から 10 枚ほど）が毎フレーム描き直しに
/// ならない広さで、VRAM を GB 単位では取らない値にする。動くページの全フレームはレンダラーのキャッシュが持つ。
/// `web.cef` の種類はプロセスで 1 つ（`mm-runtime` が組み立ての間で共有する）なので、この上限もプロセスで 1 つで、
/// レンダラーのフレームのキャッシュ（既定 1 GiB、並行の書き出しのワーカーは 1 本 256 MiB）に足して使う。
/// CPU の画像も同じ上限で数える（VRAM を使わないが、RAM を際限なく取らない）。
const KEPT_BYTES: usize = 384 << 20;
/// GPU のテクスチャ（`Rgba16Float`）の 1 画素の大きさ。
const GPU_PIXEL_BYTES: usize = 8;
/// CPU の画像（`f32` が 4 つ）の 1 画素の大きさ。
const CPU_PIXEL_BYTES: usize = 16;

/// 画素を決める入力の組。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameKey {
    page: u64,
    data: String,
    time: RationalTime,
    size: Size,
    scale_bits: u32,
}

impl FrameKey {
    /// 頼みから。
    pub fn of(req: &FrameRequest<'_>) -> Self {
        Self {
            page: req.page.hash,
            data: req.page.data_text.clone(),
            time: req.time,
            size: req.size,
            scale_bits: req.scale.to_bits(),
        }
    }
}

/// キャッシュしたフレーム。
#[derive(Clone)]
pub enum Cached {
    /// 中間の形式のテクスチャ。作ったデバイスが作り直されたら使わない。
    Gpu {
        texture: wgpu::Texture,
        ctx: Weak<GpuContext>,
    },
    /// CPU の画像。
    Cpu(Arc<RgbaImage>),
}

impl Cached {
    /// 持つ画素の大きさ（バイト）。
    fn bytes(&self, size: Size) -> usize {
        let per_pixel = match self {
            Self::Gpu { .. } => GPU_PIXEL_BYTES,
            Self::Cpu(_) => CPU_PIXEL_BYTES,
        };
        size.area() * per_pixel
    }
}

/// 新しい順の表。
#[derive(Default)]
pub struct FrameCache {
    entries: Vec<(FrameKey, Cached)>,
}

impl FrameCache {
    /// 引く。ヒットしたものは先頭へ動かす。
    pub fn get(&mut self, key: &FrameKey) -> Option<Cached> {
        let i = self.entries.iter().position(|(k, _)| k == key)?;
        let hit = self.entries.remove(i);
        let value = hit.1.clone();
        self.entries.insert(0, hit);
        Some(value)
    }

    /// キャッシュする。大きさの合計が上限を超えたら古いものから捨てる（最新の 1 枚は必ず残す）。
    pub fn put(&mut self, key: FrameKey, value: Cached) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.insert(0, (key, value));
        let mut kept = 0usize;
        let mut keep = 0usize;
        for (k, v) in &self.entries {
            kept += v.bytes(k.size);
            if kept > KEPT_BYTES && keep > 0 {
                break;
            }
            keep += 1;
        }
        self.entries.truncate(keep);
    }

    /// `ctx` 以外のデバイスで作ったテクスチャを捨てる。テクスチャは wgpu のデバイスを持ち続けるので、
    /// 作り直した後に残すと古いデバイスとその VRAM が解放されない。
    pub fn drop_other_devices(&mut self, ctx: &Arc<GpuContext>) {
        self.entries.retain(|(_, value)| match value {
            Cached::Gpu { ctx: made, .. } => made.upgrade().is_some_and(|c| Arc::ptr_eq(&c, ctx)),
            Cached::Cpu(_) => true,
        });
    }
}

impl std::fmt::Debug for FrameCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCache")
            .field("len", &self.entries.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use mm_types::ParamValue;

    use super::*;
    use crate::page::{Page, HTML_PARAM};

    fn key(page: &Page, seconds: i64) -> FrameKey {
        FrameKey::of(&FrameRequest {
            page,
            time: RationalTime::seconds(seconds),
            size: Size::new(4, 4),
            scale: 1.0,
        })
    }

    fn context() -> Option<Arc<GpuContext>> {
        match GpuContext::new() {
            Ok(ctx) => Some(ctx),
            Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_none() => {
                eprintln!("GPU が無いのでスキップする: {e}");
                None
            }
            Err(e) => panic!("MM_REQUIRE_GPU があるのに GPU が無い: {e}"),
        }
    }

    #[test]
    fn 大きさの合計が上限を超えたら古いものから捨てる() {
        let mut params = mm_types::Params::new();
        params.set(HTML_PARAM, ParamValue::Text("<p>x</p>".into()));
        let page = Page::read_with(&params, false).unwrap();
        let image = Arc::new(RgbaImage::transparent(Size::new(1, 1)));
        let key_at = |seconds, size| {
            FrameKey::of(&FrameRequest {
                page: &page,
                time: RationalTime::seconds(seconds),
                size,
                scale: 1.0,
            })
        };
        let mut cache = FrameCache::default();
        // 1080p の CPU の画像は 1 枚 32 MiB なので、12 枚で上限に届く。
        let full_hd = Size::new(1920, 1080);
        for n in 0..12 {
            cache.put(key_at(n, full_hd), Cached::Cpu(image.clone()));
        }
        assert!(cache.get(&key_at(0, full_hd)).is_some(), "12 枚は入る");
        cache.put(key_at(12, full_hd), Cached::Cpu(image.clone()));
        assert!(
            cache.get(&key_at(1, full_hd)).is_none(),
            "最後に使ったのが最も古い 1 枚が出る"
        );
        assert!(
            cache.get(&key_at(0, full_hd)).is_some(),
            "引いた 1 枚は新しい側へ動いて残る"
        );
        let huge = Size::new(16384, 16384);
        cache.put(key_at(99, huge), Cached::Cpu(image));
        assert!(
            cache.get(&key_at(99, huge)).is_some(),
            "上限より大きい 1 枚も残す"
        );
        assert!(cache.get(&key_at(0, full_hd)).is_none(), "ほかは全部出る");
    }

    #[test]
    fn 作り直した後は前のデバイスのテクスチャを捨てる() {
        let (Some(old), Some(new)) = (context(), context()) else {
            return;
        };
        let mut params = mm_types::Params::new();
        params.set(HTML_PARAM, ParamValue::Text("<p>x</p>".into()));
        let page = Page::read_with(&params, false).unwrap();
        let texture = old.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: mm_gpu::FRAME_FORMAT,
            usage: wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut cache = FrameCache::default();
        let gpu = Cached::Gpu {
            texture,
            ctx: Arc::downgrade(&old),
        };
        cache.put(key(&page, 0), gpu);
        let image = Arc::new(RgbaImage::transparent(Size::new(4, 4)));
        cache.put(key(&page, 1), Cached::Cpu(image));
        cache.drop_other_devices(&old);
        assert_eq!(cache.entries.len(), 2, "同じデバイスなら残す");
        cache.drop_other_devices(&new);
        assert!(
            cache.get(&key(&page, 0)).is_none(),
            "前のデバイスのものは捨てる"
        );
        assert!(cache.get(&key(&page, 1)).is_some(), "CPU の画像は残す");
    }
}
