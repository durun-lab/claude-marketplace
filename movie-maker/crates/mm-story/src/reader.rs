//! Story の読み出し本体。プロジェクトのデータは呼び出しごとに受け取る。

use std::path::{Path, PathBuf};

use mm_doc::{Asset, Layer, Project, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_ingest::CacheLayout;
use mm_store::Store;
use mm_types::{AssetId, CompositionId, Id, RationalTime};

use crate::model::{
    Overview, StoryAsset, StoryHit, StorySequence, StoryTranscript, StoryWord, WordRange,
};
use crate::words::{find_in, range_end, sidecar_words, BaseWord};

/// 全文検索の語の長さ（文字数）の上限。`mm search` と同じ。
pub const QUERY_MAX_CHARS: usize = mm_store::RUN_EXTRA_CHARS;

/// レイヤーのシーケンス。シーケンスのレイヤーでなければ `None`。
pub fn layer_sequence<'a>(project: &'a Project, layer: &Layer) -> Option<&'a Sequence> {
    if layer.source.kind != SEQUENCE_KIND {
        return None;
    }
    let id = Id::new(layer.source.text_param(SEQUENCE_PARAM)?);
    project.sequence(&id).ok()
}

/// 1 つのプロジェクトの文字起こしの置き場（`project.mmdb` と `cache/`）を持つ読み手。
pub struct StoryReader {
    store_dir: Option<PathBuf>,
    cache: CacheLayout,
}

impl StoryReader {
    /// `project` は開いたプロジェクトのパス、`cache_root` はその `cache/`。
    pub fn new(project: &Path, cache_root: PathBuf) -> Self {
        Self::with_store_dir(mm_store::store_dir(project), cache_root)
    }

    /// `project.mmdb` のあるフォルダーを直に渡す。
    pub fn with_store_dir(store_dir: Option<PathBuf>, cache_root: PathBuf) -> Self {
        Self {
            store_dir,
            cache: CacheLayout::new(cache_root),
        }
    }

    /// 取り込みの付属ファイルの置き場。
    pub fn cache(&self) -> &CacheLayout {
        &self.cache
    }

    fn asset_of(project: &Project, id: &str) -> Result<Asset, String> {
        project
            .asset(&Id::new(id))
            .cloned()
            .map_err(|_| format!("素材が無い: {id}"))
    }

    /// 素材と、シーケンスの一覧。`comp` はいま開いているコンポジション。
    pub fn overview(&self, project: &Project, comp: &CompositionId) -> Overview {
        let open_sequence = project.composition(comp).ok().and_then(|c| {
            c.layers
                .iter()
                .find_map(|l| layer_sequence(project, l))
                .map(|s| s.id.to_string())
        });
        let assets = project
            .assets
            .iter()
            .map(|a| StoryAsset {
                id: a.id.to_string(),
                name: file_name(&a.path),
                duration: a.duration.to_f64(),
                has_transcript: self.has_transcript(a),
            })
            .collect();
        let sequences = project
            .sequences
            .iter()
            .map(|s| StorySequence {
                id: s.id.to_string(),
                cuts: s.cuts.len(),
                duration: mm_eval::sequence_duration(s).map_or(0.0, RationalTime::to_f64),
            })
            .collect();
        Overview {
            assets,
            sequences,
            open_sequence,
        }
    }

    fn has_transcript(&self, asset: &Asset) -> bool {
        self.cache
            .sidecar(&asset.content_hash, "transcript")
            .is_file()
            || self.store_has(&asset.id)
    }

    fn store_has(&self, asset: &AssetId) -> bool {
        self.open_store()
            .is_some_and(|s| s.has_transcript(asset).unwrap_or(false))
    }

    fn open_store(&self) -> Option<Store> {
        Store::open_read_only(self.store_dir.as_deref()?).ok()
    }

    /// 素材の文字起こしの単語に、`project` の修正を重ねたもの。文字起こしが無ければ空。
    pub fn words(&self, project: &Project, asset_id: &str) -> Result<Vec<StoryWord>, String> {
        let asset = Self::asset_of(project, asset_id)?;
        let base = self.base_words(&asset)?;
        Ok(base
            .into_iter()
            .map(|w| {
                let edit = project.word_edit(&asset.id, &Id::new(&w.id));
                StoryWord {
                    id: w.id,
                    segment: w.segment,
                    start: w.start,
                    end: w.end,
                    text: w.text,
                    fixed: edit.and_then(|e| e.text.clone()),
                    deleted: edit.is_some_and(|e| e.deleted),
                }
            })
            .collect())
    }

    fn base_words(&self, asset: &Asset) -> Result<Vec<BaseWord>, String> {
        if let Some(store) = self.open_store() {
            if store.has_transcript(&asset.id).unwrap_or(false) {
                let words = store
                    .transcript_words(&asset.id)
                    .map_err(|e| e.to_string())?;
                return Ok(words
                    .into_iter()
                    .map(|w| BaseWord {
                        id: w.word.to_string(),
                        segment: w.segment,
                        start: w.start,
                        end: w.end,
                        text: w.text,
                    })
                    .collect());
            }
        }
        match mm_ingest::sidecar::load_transcript(&self.cache, &asset.content_hash) {
            Ok(Some(t)) => Ok(sidecar_words(&asset.content_hash, &t)),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(format!("文字起こしを読めない: {e}")),
        }
    }

    /// 素材の文字起こしと、その素材の話題・選別。
    pub fn transcript(&self, project: &Project, asset_id: &str) -> Result<StoryTranscript, String> {
        let asset = Self::asset_of(project, asset_id)?;
        Ok(StoryTranscript {
            asset: asset_id.to_owned(),
            words: self.words(project, asset_id)?,
            topics: of_asset(&project.bins.topics, |t| &t.asset, &asset.id),
            selections: of_asset(&project.bins.selections, |s| &s.asset, &asset.id),
        })
    }

    /// 単語 `first` から `last`（どちらが先でもよい）までの時刻。終わりは [`range_end`] で詰める。
    pub fn range(
        &self,
        project: &Project,
        asset: &str,
        first: &str,
        last: &str,
    ) -> Result<WordRange, String> {
        let words = self.words(project, asset)?;
        let find = |id: &str| {
            words
                .iter()
                .position(|w| w.id == id)
                .ok_or_else(|| format!("単語が無い: {id}"))
        };
        let (a, b) = (find(first)?, find(last)?);
        let (a, b) = (a.min(b), a.max(b));
        Ok(WordRange {
            start: words[a].start,
            end: range_end(&words, b),
        })
    }

    /// 全文検索。`mm search` と同じ規則（単語の境をまたいで探す、消した単語は外す、直した文字で探す）で、
    /// 保存前の修正も見えるよう、パネルに出す単語と同じ列を探す。
    pub fn search(
        &self,
        project: &Project,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StoryHit>, String> {
        let q = query.trim();
        let chars = q.chars().count();
        if chars == 0 || chars > QUERY_MAX_CHARS {
            return Err(format!(
                "検索の語は 1 文字以上 {QUERY_MAX_CHARS} 文字まで: {query:?}"
            ));
        }
        let mut hits = Vec::new();
        for asset in &project.assets {
            if hits.len() >= limit {
                break;
            }
            let words = self.words(project, asset.id.as_str())?;
            hits.extend(find_in(asset.id.as_str(), &words, q, limit - hits.len()));
        }
        Ok(hits)
    }
}

fn of_asset<T: Clone>(items: &[T], asset_of: impl Fn(&T) -> &AssetId, asset: &AssetId) -> Vec<T> {
    items
        .iter()
        .filter(|i| asset_of(i) == asset)
        .cloned()
        .collect()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}
