//! `Ui` の帯のサムネイル。描く並びを作って GPU に渡し、サムネイルの読み込みの依頼と結果の受け渡しの口を持つ。

use std::sync::Arc;

use mm_ui_layout::timeline_thumbs;

use super::Ui;
use crate::thumbs::ThumbRenderer;

/// サムネイルの状態の数（測定と試験用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThumbStats {
    /// アトラスのスロットに入っているサムネイルの数。
    pub resident: usize,
    /// 直近のフレームで描いた数。
    pub drawn: u32,
}

impl Ui {
    /// 描きたいのにスロットに無いサムネイルの鍵（素材のコンテンツハッシュ）。1 度頼んだ鍵は結果が届くまで返さない。
    /// アプリが別のスレッドで読み、結果を [`Ui::set_thumb`] に渡す。
    pub fn take_thumb_requests(&mut self) -> Vec<Arc<str>> {
        self.thumbs.take_requests()
    }

    /// 読み込んだサムネイルをスロットに書く。`cell` は [`crate::cell_from_rgb`] の大きさの RGBA8。読めなかったときは `None`
    /// （モデルが置き換わるまで頼み直さない）。
    pub fn set_thumb(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        cell: Option<&[u8]>,
    ) {
        self.thumbs.set(device, queue, key, cell);
    }

    /// アトラスを空から作り直す。サーフェスを作り直したとき（`Lost`）に呼ぶ。デバイスごと失われていてもよいよう、
    /// スロットの割り当てもテクスチャも捨て、見えているサムネイルは次のフレームで頼み直す。
    pub fn reset_thumbs(&mut self) {
        self.thumbs = ThumbRenderer::new(self.format);
        self.thumbs_drawn = 0;
    }

    /// サムネイルの状態の数。
    pub fn thumb_stats(&self) -> ThumbStats {
        ThumbStats {
            resident: self.thumbs.resident(),
            drawn: self.thumbs_drawn,
        }
    }

    /// 見えている帯のサムネイルを GPU に渡し、描く数を返す。
    pub(super) fn prepare_thumbs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> u32 {
        let t = self.layout.timeline;
        let state = self.paint_state();
        let vp = self.controller.viewport();
        let mut quads = timeline_thumbs(&self.model, &vp, &state);
        for q in &mut quads {
            q.rect = self.scale.rect_to_physical(q.rect.translated(t.x, t.y));
        }
        let count = self.thumbs.prepare(device, queue, &quads, self.size);
        self.thumbs_drawn = count;
        count
    }
}
