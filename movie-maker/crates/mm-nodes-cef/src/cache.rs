//! 描いたフレームのキャッシュ。ページ、パラメータ、時刻、大きさ、倍率の組で引く。
//! 静止のページは時刻を 0 に揃えるので、パラメータの組ごとに 1 枚だけ描けば済む。

use std::sync::{Arc, Weak};

use mm_gpu::GpuContext;
use mm_types::{RationalTime, RgbaImage, Size};

use crate::page::FrameRequest;

/// キャッシュするフレームの数。1080p の `Rgba16Float` は 1 枚 16 MB なので、静止の部品が数個並ぶ程度に留める。
/// 動くページの全フレームはレンダラーのキャッシュが持つ。
const KEPT_FRAMES: usize = 6;

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

    /// キャッシュする。上限を超えたら古いものから捨てる。
    pub fn put(&mut self, key: FrameKey, value: Cached) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.insert(0, (key, value));
        self.entries.truncate(KEPT_FRAMES);
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
