//! Timeline の帯の先頭のサムネイルの読み込み。キャッシュの `thumbs/`（Media のパネルと同じ JPEG）を別のスレッドで読み、
//! 描画スレッドは届いた分だけを `Ui` に渡す。描画を待たせず、届くまではサムネイルなしで描く。
//! `Ui` を作り直したとき（サーフェスの作り直し）はこれも作り直し、新しいアトラスにサムネイルを詰め直す。

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use mm_ingest::CacheLayout;
use mm_ui_wgpu::{cell_from_rgb, Ui};
use zune_jpeg::JpegDecoder;

use super::Gpu;

/// 1 フレームにスロットへ書くサムネイルの数の上限。一度に大量に届いても、フレームを延ばさない。
const UPLOADS_PER_FRAME: usize = 16;

/// 読み込み 1 件の結果。鍵と、スロットの大きさの RGBA8（読めなければ `None`）。
type Loaded = (Arc<str>, Option<Vec<u8>>);

/// `Ui` が頼んだサムネイルを読むスレッドと、その結果を受ける口。落とすとスレッドが止まる。
pub(super) struct ThumbSync {
    requests: Sender<Arc<str>>,
    loaded: Receiver<Loaded>,
}

impl ThumbSync {
    /// `cache` は `<project>/cache`。
    pub(super) fn new(cache: CacheLayout) -> Self {
        let (requests, wanted) = channel::<Arc<str>>();
        let (done, loaded) = channel::<Loaded>();
        std::thread::Builder::new()
            .name("thumbs".into())
            .spawn(move || {
                for key in wanted {
                    let cell = load_cell(&cache.thumbnail(&key));
                    if done.send((key, cell)).is_err() {
                        return;
                    }
                }
            })
            .expect("thumbs thread");
        Self { requests, loaded }
    }

    /// `ui` が新たに頼んだサムネイルを読むスレッドに渡し、読み終えたサムネイルを `ui` に渡す。
    pub(super) fn refresh(&mut self, gpu: &Gpu, ui: &mut Ui) {
        for key in ui.take_thumb_requests() {
            let _ = self.requests.send(key);
        }
        for (key, cell) in self.loaded.try_iter().take(UPLOADS_PER_FRAME) {
            ui.set_thumb(&gpu.device, &gpu.queue, &key, cell.as_deref());
        }
    }
}

/// JPEG のファイルを、スロットの大きさ（16:9 の中央を切り出して縮めた RGBA8）にして読む。
/// ファイルが無い、壊れている、JPEG が持つ色の数が RGB でないときは `None`。
fn load_cell(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let mut decoder = JpegDecoder::new(bytes.as_slice());
    let rgb = decoder.decode().ok()?;
    let (w, h) = decoder.dimensions()?;
    cell_from_rgb(u32::try_from(w).ok()?, u32::try_from(h).ok()?, &rgb)
}

#[cfg(test)]
mod tests;
