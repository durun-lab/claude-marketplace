//! `Ui` の帯のサムネイル。描く並びを作って GPU に渡し、読み込みを頼む関数と結果を受ける関数を持つ。
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

/// 読めなかったので帯から外したキー（行、カット、キー）。モデルを置き換えるたびに作り直し、読み直して読めたら帯に戻す。
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

    /// 外したキーのうち `key` を、`model` の元の帯に戻す。
    fn restore_key(&mut self, model: &mut TimelineModel, key: &str) {
        self.0.retain(|(i, cut, k)| {
            if &**k != key {
                return true;
            }
            if let Some(thumb) = thumb_of(model, *i, *cut) {
                *thumb = Some(k.clone());
            }
            false
        });
    }
}

/// 行 `i` の帯（`cut` があればそのカット）のサムネイルのキー。
fn thumb_of(
    model: &mut TimelineModel,
    i: usize,
    cut: Option<usize>,
) -> Option<&mut Option<Arc<str>>> {
    let item = model.items.get_mut(i)?;
    match cut {
        None => Some(&mut item.thumb),
        Some(c) => item.cuts.get_mut(c).map(|cut| &mut cut.thumb),
    }
}

impl Ui {
    /// 描きたいのにスロットに無いサムネイルのキー（素材のコンテンツハッシュ）。1 度頼んだキーは結果が届くまで返さない。
    /// アプリが別のスレッドで読み、結果を [`Ui::set_thumb`] に渡す。
    pub fn take_thumb_requests(&mut self) -> Vec<Arc<str>> {
        self.thumbs.take_requests()
    }

    /// 読み込んだサムネイルをスロットに書く。`image` は [`crate::slot_image_from_rgb`] が作るスロットの画像。
    /// 読めなかったとき（`None`）はそのキーを帯から外し、[`Ui::retry_thumbs`] まで頼み直さない。
    /// 外したキーが読み直しで読めたら、帯に戻す。
    pub fn set_thumb(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        image: Option<&[u8]>,
    ) {
        self.thumbs.set(device, queue, key, image);
        if self.thumbs.is_missing(key) {
            self.dropped_thumbs
                .drop_where(&mut self.model, |k| k == key);
        } else {
            self.dropped_thumbs.restore_key(&mut self.model, key);
        }
    }

    /// 読めなかったサムネイルを読み直す。読めたキーだけを帯に戻すので、読めないままのキーの帯の名前は動かない。
    /// 取り込みが素材の出力を作り終えたとき（キャッシュにサムネイルができたかもしれない）に呼ぶ。
    pub fn retry_thumbs(&mut self) {
        self.thumbs.retry_missing();
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
