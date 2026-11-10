//! Timeline の帯の先頭のサムネイルの読み込み。キャッシュの `thumbs/`（Media のパネルと同じ JPEG）を別のスレッドで読み、
//! 描画スレッドは届いた分だけを `Ui` に渡す。描画を待たせず、届くまではサムネイルなしで描く。
//! 取り込みが素材の出力を作り終えたら（`Shared::ingest_serial`）、読めなかったサムネイルを `Ui` に読み直させる。

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use image::ImageFormat;
use mm_ingest::CacheLayout;
use mm_ui_wgpu::{slot_image_from_rgb, Ui};

use super::Gpu;
use crate::win::state::Shared;

/// 1 フレームにスロットへ書くサムネイルの数の上限。一度に大量に届いても、フレームを延ばさない。
const UPLOADS_PER_FRAME: usize = 16;

/// 読み込み 1 件の結果。キーと、スロットの画像（読めなければ `None`）。
type Loaded = (Arc<str>, Option<Vec<u8>>);

/// `Ui` が頼んだサムネイル（キーとキャッシュのファイル）を読むスレッドへの送り手と、その結果の受け手。
/// 破棄するとスレッドが止まる。
pub(super) struct ThumbSync {
    requests: Sender<(Arc<str>, PathBuf)>,
    loaded: Receiver<Loaded>,
    /// 最後に見た `Shared::ingest_serial`。
    seen_ingest: u64,
}

impl ThumbSync {
    /// 作った時点より前の取り込みは、`Ui` が最初に頼むときに読むので数えない。
    pub(super) fn new(shared: &Shared) -> Self {
        let (requests, wanted) = channel::<(Arc<str>, PathBuf)>();
        let (done, loaded) = channel::<Loaded>();
        std::thread::Builder::new()
            .name("thumbs".into())
            .spawn(move || {
                for (key, path) in wanted {
                    let image = load_slot_image(&path);
                    if done.send((key, image)).is_err() {
                        return;
                    }
                }
            })
            .expect("thumbs thread");
        Self {
            requests,
            loaded,
            seen_ingest: shared.ingest_serial.load(Ordering::SeqCst),
        }
    }

    /// 取り込みが進んでいれば `ui` に読み直させ、`ui` が新たに頼んだサムネイルを読むスレッドに渡し、
    /// 読み終えたサムネイルを `ui` に渡す。キャッシュのフォルダーが決まるまでは頼まない。
    pub(super) fn refresh(&mut self, shared: &Shared, gpu: &Gpu, ui: &mut Ui) {
        let ingest = shared.ingest_serial.load(Ordering::SeqCst);
        if ingest != self.seen_ingest {
            self.seen_ingest = ingest;
            ui.retry_thumbs();
        }
        if let Some(root) = shared.cache_dir.get() {
            self.send(root, ui.take_thumb_requests());
        }
        for (key, image) in self.loaded.try_iter().take(UPLOADS_PER_FRAME) {
            ui.set_thumb(&gpu.device, &gpu.queue, &key, image.as_deref());
        }
    }

    /// サムネイル `keys` を、キャッシュ（`root` は `<project>/cache`）のファイルと一緒に読むスレッドへ渡す。
    fn send(&self, root: &Path, keys: Vec<Arc<str>>) {
        if keys.is_empty() {
            return;
        }
        let cache = CacheLayout::new(root);
        for key in keys {
            let path = cache.thumbnail(&key);
            let _ = self.requests.send((key, path));
        }
    }
}

/// JPEG のファイルを読み、スロットの画像（16:9 の中央を切り出して縮めた RGBA8 とミップレベル）にする。
/// グレースケールの JPEG も RGB にそろえる。ファイルが無いか壊れているときは `None`。
fn load_slot_image(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let rgb = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
        .ok()?
        .into_rgb8();
    slot_image_from_rgb(rgb.width(), rgb.height(), rgb.as_raw())
}

#[cfg(test)]
mod tests;
