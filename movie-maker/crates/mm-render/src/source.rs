//! 素材の供給元。デコーダーを時刻で引ける形にし、同じフレームの連続した要求はデコードし直さない。
//!
//! 同じ素材を 1 つのフレームで別の位置から読む（入れ子で同じ素材を時刻をずらして重ねる）と、デコーダーが 1 つでは
//! 毎回シークし直す。そこで素材ごとに [`MAX_DECODERS_PER_ASSET`] 個までデコーダーを持ち、次のフレームを
//! 読むだけで済むものを選ぶ。デコーダーを開くのはシークより重いので、そのフレームでまだ使っていないものがあれば
//! シークで済ませ、全部が使用中のときだけ新しく開く。開くのを待つ上限があれば、開くのはバックグラウンドのスレッド
//! （`opening`）。

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use mm_audio_api::AudioBlock;
use mm_decode_api::{DecodedFrame, Decoder, FrameSource, MediaInfo};
use mm_doc::Project;
use mm_types::{AssetId, MmError, ParamValue, Params, RationalTime, Result, TimeRange};

use opening::Opening;
pub use opening::{Opener, PendingSources};

mod opening;

/// 素材ごとに持つデコーダーの上限。重ねる数がこれを超えたら、最も前に使ったものをシークし直す。
pub(crate) const MAX_DECODERS_PER_ASSET: usize = 4;

/// この数のフレームの間に使わなかったデコーダーは閉じる。字幕の画像のように短い間だけ使う素材が多いと、
/// 開いたままのデコーダー（直前のフレームの画像を持つ）でメモリが増え続けるため。30 fps で 10 秒。
pub(crate) const IDLE_FRAMES: u64 = 300;

/// `Decoder` を `FrameSource` にする。直前のフレームを保持し、次のフレームはシークせずに読む。
pub struct DecoderSource {
    decoder: Box<dyn Decoder>,
    last: Option<DecodedFrame>,
    /// 最後に使ったフレームの番号（`SourcePool` の数え）。
    used: u64,
}

impl DecoderSource {
    /// デコーダーをラップする。
    pub fn new(decoder: Box<dyn Decoder>) -> Self {
        Self {
            decoder,
            last: None,
            used: 0,
        }
    }

    /// `t` のフレームを、シークせずに返せるか（同じフレームか、直前のフレームの次）。
    fn reaches(&self, t: RationalTime) -> bool {
        let index = self.decoder.info().frame_index_at(t);
        self.last
            .as_ref()
            .is_some_and(|l| l.index == index || l.index + 1 == index)
    }

    fn read_next(&mut self) -> Result<Option<DecodedFrame>> {
        self.decoder.next_frame()
    }
}

impl FrameSource for DecoderSource {
    fn info(&self) -> &MediaInfo {
        self.decoder.info()
    }

    fn frame_at(&mut self, t: RationalTime) -> Result<DecodedFrame> {
        let index = self.decoder.info().frame_index_at(t);
        if let Some(last) = self.last.as_ref().filter(|l| l.index == index) {
            return Ok(last.clone());
        }
        // 直前の次のフレームはそのまま読む。番号が合わなければ（可変フレームレートなど）シークし直す。
        if self.last.as_ref().is_some_and(|l| l.index + 1 == index) {
            if let Some(frame) = self.read_next()?.filter(|f| f.index == index) {
                self.last = Some(frame.clone());
                return Ok(frame);
            }
        }
        mm_trace::counter!("render.source_seeks", 1);
        let _span = tracing::debug_span!("render.seek_source").entered();
        self.decoder.seek(t)?;
        let frame = self
            .read_next()?
            .ok_or_else(|| MmError::NotFound(format!("フレーム {index}（t = {t}）")))?;
        self.last = Some(frame.clone());
        Ok(frame)
    }

    fn audio_in(&mut self, range: TimeRange) -> Result<AudioBlock> {
        self.decoder.decode_audio(range)
    }
}

/// 素材を引いた結果。
pub enum Footage<'s> {
    /// パラメータに素材が無い。
    Absent,
    /// バックグラウンドのスレッドで開いている途中。このフレームでは描かない。
    Pending,
    /// 開いた素材と、デコーダーの実装の名前（`Decoder::backend_id`。名乗らなければファクトリーの名前）。
    Ready(&'s mut (dyn FrameSource + 's), &'static str),
}

/// 開いた素材のキャッシュ。素材ごとに開いたデコーダーを使い回す。
#[derive(Default)]
pub struct SourcePool {
    sources: HashMap<AssetId, Vec<DecoderSource>>,
    opening: Opening,
    /// 描いているフレームの数え。同じフレームの中で使用中のデコーダーを見分ける。
    frame: u64,
}

impl SourcePool {
    /// 1 フレームで素材を開くのを待つ上限を決める。`None`（既定）は開き終わるまで待つ。
    pub fn set_open_wait(&mut self, wait: Option<Duration>) {
        self.opening.set_wait(wait);
    }

    /// 素材を開いている途中のために描かなかったレイヤーの数と、開き終わった数。
    pub fn pending(&self) -> PendingSources {
        self.opening.pending()
    }

    /// このフレームで、素材を開いている途中のために描かなかったレイヤーの数。
    pub fn pending_layers(&self) -> usize {
        self.opening.pending_layers()
    }

    /// 全部閉じる。素材の内容が変わったとき。
    pub fn clear(&mut self) {
        self.sources.clear();
        self.opening.clear();
    }

    /// 次のフレームを描き始める。前のフレームで使ったデコーダーも、次のフレームでは空きになる。
    /// [`IDLE_FRAMES`] ごとに、その間に使わなかったデコーダーと、求められなかった開いている途中のものを閉じる。
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.opening.begin_frame();
        if self.frame.is_multiple_of(IDLE_FRAMES) {
            let frame = self.frame;
            for opened in self.sources.values_mut() {
                opened.retain(|s| frame - s.used <= IDLE_FRAMES);
            }
            self.sources.retain(|_, opened| !opened.is_empty());
            self.opening.drop_idle(frame, IDLE_FRAMES);
        }
    }

    /// 素材を読んでいるデコーダーの実装の名前。開いていないか、実装が名乗らなければ `None`。
    pub fn backend(&self, asset: &AssetId) -> Option<&'static str> {
        self.sources
            .get(asset)?
            .iter()
            .find_map(|s| s.decoder.backend_id())
    }

    /// 素材 ID が `ids` にあるものだけ閉じる。素材の中身やパスが変わったとき。
    pub fn forget(&mut self, ids: &[String]) {
        self.sources
            .retain(|id, _| !ids.iter().any(|i| i == id.as_str()));
        self.opening.forget(ids);
    }

    /// 評価済みのパラメータの `AssetRef` の素材を、素材の時刻 `t` を読むのに向いたデコーダーで返す（無ければ開く）。
    /// `AssetRef` が無いか空なら `None`。開いている途中（音はその区間を無音にする）も `None`。
    pub fn footage_for<'s>(
        &'s mut self,
        opener: &Opener,
        project: &Project,
        params: &Params,
        t: RationalTime,
    ) -> Result<Option<&'s mut (dyn FrameSource + 's)>> {
        match self.footage_with_backend(opener, project, params, t)? {
            Footage::Ready(source, _) => Ok(Some(source)),
            Footage::Absent | Footage::Pending => Ok(None),
        }
    }

    /// [`Self::footage_for`] と同じで、デコーダーの実装の名前も返し（フレームのキャッシュのキーに入れる）、
    /// 開いている途中を素材が無いことと分けて返す。
    pub fn footage_with_backend<'s>(
        &'s mut self,
        opener: &Opener,
        project: &Project,
        params: &Params,
        t: RationalTime,
    ) -> Result<Footage<'s>> {
        let Some(asset_id) = params.0.values().find_map(ParamValue::as_asset) else {
            return Ok(Footage::Absent);
        };
        if asset_id.as_str().is_empty() {
            return Ok(Footage::Absent);
        }
        let frame = self.frame;
        let opened = self.sources.entry(asset_id.clone()).or_default();
        let slot = match pick(opened, t, frame) {
            Some(i) => i,
            None => {
                let path = Path::new(&project.asset(asset_id)?.path);
                let Some(decoder) = self.opening.open(opener, (asset_id, path), frame)? else {
                    return Ok(Footage::Pending);
                };
                mm_trace::counter!("render.source_opens", 1);
                opened.push(DecoderSource::new(decoder));
                opened.len() - 1
            }
        };
        let source = &mut opened[slot];
        source.used = frame;
        let backend = source.decoder.backend_id().unwrap_or(opener.decoders.id());
        Ok(Footage::Ready(
            source as &mut (dyn FrameSource + 's),
            backend,
        ))
    }
}

/// `t` を読むデコーダーの位置。読み進めるだけで済むもの、このフレームで空いているもの、の順に選ぶ。
/// 全部が使用中で上限に満たなければ `None`（新しく開く）。上限なら最も前に使ったもの。
fn pick(opened: &[DecoderSource], t: RationalTime, frame: u64) -> Option<usize> {
    if let Some(i) = opened.iter().position(|s| s.reaches(t)) {
        return Some(i);
    }
    let idle = opened
        .iter()
        .enumerate()
        .filter(|(_, s)| s.used != frame)
        .min_by_key(|(_, s)| s.used)
        .map(|(i, _)| i);
    if idle.is_some() || opened.len() < MAX_DECODERS_PER_ASSET {
        return idle;
    }
    opened
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.used)
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests;
