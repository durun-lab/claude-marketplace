//! `Ui` の帯のサムネイル。描く並びを作って GPU に渡し、サムネイルの読み込みの依頼と結果の受け渡しの口を持つ。
//! 読めなかったサムネイル（音だけの素材、キャッシュの無い素材）のキーは帯から外し、名前をサムネイルの幅だけ右へずらさない。

use std::sync::Arc;

use mm_ui_layout::{timeline_thumbs, TimelineModel};

use super::Ui;

/// サムネイルの状態の数（測定と試験用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThumbStats {
    /// アトラスのスロットに入っているサムネイルの数。
    pub resident: usize,
    /// 直近のフレームで描いた数。
    pub drawn: u32,
    /// 読めなかったので帯から外したキーの数（帯ごとに数える）。
    pub dropped: usize,
}

/// 読めなかったので帯から外したキー（行、カット、キー）。モデルを置き換えるたびに作り直し、取り込みを終えたら帯に戻す。
#[derive(Debug, Default)]
pub(super) struct DroppedThumbs(Vec<(usize, Option<usize>, Arc<str>)>);

impl DroppedThumbs {
    /// `model` の帯のキーのうち、`missing` が真のものを外して覚える。
    fn drop_where(&mut self, model: &mut TimelineModel, missing: impl Fn(&str) -> bool) {
        for (i, item) in model.items.iter_mut().enumerate() {
            if let Some(key) = item.thumb.take_if(|k| missing(k)) {
                self.0.push((i, None, key));
            }
            for (c, cut) in item.cuts.iter_mut().enumerate() {
                if let Some(key) = cut.thumb.take_if(|k| missing(k)) {
                    self.0.push((i, Some(c), key));
                }
            }
        }
    }

    /// 外したキーを `model` の帯に戻す。
    fn restore(&mut self, model: &mut TimelineModel) {
        for (i, cut, key) in self.0.drain(..) {
            let Some(item) = model.items.get_mut(i) else {
                continue;
            };
            let thumb = match cut {
                None => &mut item.thumb,
                Some(c) => match item.cuts.get_mut(c) {
                    Some(cut) => &mut cut.thumb,
                    None => continue,
                },
            };
            *thumb = Some(key);
        }
    }
}

impl Ui {
    /// 描きたいのにスロットに無いサムネイルのキー（素材のコンテンツハッシュ）。1 度頼んだキーは結果が届くまで返さない。
    /// アプリが別のスレッドで読み、結果を [`Ui::set_thumb`] に渡す。
    pub fn take_thumb_requests(&mut self) -> Vec<Arc<str>> {
        self.thumbs.take_requests()
    }

    /// 読み込んだサムネイルをスロットに書く。`cell` は [`crate::cell_from_rgb`] の大きさの RGBA8。
    /// 読めなかったとき（`None`）はそのキーを帯から外し、[`Ui::retry_thumbs`] まで頼み直さない。
    pub fn set_thumb(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        cell: Option<&[u8]>,
    ) {
        self.thumbs.set(device, queue, key, cell);
        if self.thumbs.is_missing(key) {
            self.dropped_thumbs
                .drop_where(&mut self.model, |k| k == key);
        }
    }

    /// 読めなかったキーを帯に戻し、見えているものから読み直す。素材の取り込みを終えたとき（キャッシュにサムネイルが
    /// できたかもしれない）に呼ぶ。
    pub fn retry_thumbs(&mut self) {
        self.thumbs.retry_missing();
        self.dropped_thumbs.restore(&mut self.model);
    }

    /// サムネイルの状態の数。
    pub fn thumb_stats(&self) -> ThumbStats {
        ThumbStats {
            resident: self.thumbs.resident(),
            drawn: self.thumbs.drawn(),
            dropped: self.dropped_thumbs.0.len(),
        }
    }

    /// 置き換えたモデルの帯から、読めなかったキーを外す（編集のたびに名前が右へずれて戻るのを繰り返さない）。
    pub(super) fn drop_missing_thumbs(&mut self) {
        self.dropped_thumbs = DroppedThumbs::default();
        if !self.thumbs.has_missing() {
            return;
        }
        let thumbs = &self.thumbs;
        self.dropped_thumbs
            .drop_where(&mut self.model, |k| thumbs.is_missing(k));
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
        self.thumbs.prepare(device, queue, &quads, self.size)
    }
}

#[cfg(test)]
mod tests;
