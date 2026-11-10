//! Story パネルのアプリ側の処理。文字起こしの読み出しは `mm-story`、ここはアプリの最新のプロジェクトのデータを渡す
//! 薄い層と、素材の時刻から再生位置への変換、Arrange の並べ先の決め方。Tauri のコマンドのハンドラーは `story_ipc.rs`、
//! 名前と引数の形は `ui/sdk/README.md` の「Story のコマンド」。
//!
//! アプリの操作は再生スレッドのデータに先に入るので、消した単語・直した文字は DB より新しい
//! 再生スレッドのデータを重ねる。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_doc::{CutId, Project, TopicId};
use mm_ops::{InsertTopics, Op};
use mm_story::StoryReader;
use mm_types::{CompositionId, Id, RationalTime};

pub use mm_story::{Overview, StoryHit, StoryTranscript, WordRange};

use crate::win::edit::{self, EditOutcome};
use crate::win::sequence_edit::new_sequence::NewSequence;
use crate::win::state::Shared;
use crate::win::timebase;

/// アプリ全体で 1 つの Story の状態。
pub struct StoryState {
    shared: Arc<Shared>,
    reader: StoryReader,
}

impl StoryState {
    /// `project` は開いたプロジェクトのパス、`cache_root` はその `cache/`。
    pub fn new(shared: Arc<Shared>, project: &Path, cache_root: PathBuf) -> Self {
        Self {
            shared,
            reader: StoryReader::new(project, cache_root),
        }
    }

    fn project(&self) -> Result<(Arc<Project>, mm_types::CompositionId), String> {
        let doc = self.shared.doc().ok_or("No project open")?;
        Ok((doc.project, doc.comp))
    }

    /// 素材と、シーケンスの一覧。
    pub fn overview(&self) -> Result<Overview, String> {
        let (project, comp) = self.project()?;
        Ok(self.reader.overview(&project, &comp))
    }

    /// 素材の文字起こし（いまの修正を重ねた単語の列、話題、選別）。文字起こしが無い素材は単語が空。
    pub fn transcript(&self, asset_id: &str) -> Result<StoryTranscript, String> {
        let (project, _) = self.project()?;
        self.reader.transcript(&project, asset_id)
    }

    /// 単語 `first` から `last`（どちらが先でもよい）までの時刻。
    pub fn range(&self, asset: &str, first: &str, last: &str) -> Result<WordRange, String> {
        let (project, _) = self.project()?;
        self.reader.range(&project, asset, first, last)
    }

    /// 全文検索。保存前の修正も見えるよう、パネルに出す単語と同じ列を探す。
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<StoryHit>, String> {
        let (project, _) = self.project()?;
        self.reader.search(&project, query, limit)
    }

    /// 素材の時刻 `at`（秒）が、いま開いているコンポジションの撮影素材のレイヤーか sequence のカットに入っているときの
    /// 再生位置（秒）。どこにも入っていなければ `None`。
    pub fn timeline_time(&self, asset: &str, at: f64) -> Result<Option<f64>, String> {
        let (project, comp) = self.project()?;
        let composition = project.composition(&comp).map_err(|e| e.to_string())?;
        Ok(timebase::source_to_comp(&project, composition, asset, at))
    }

    /// カット `cut` の始まりの再生位置（秒）。いま開いているコンポジションに映っていなければ `None`。
    pub fn cut_time(&self, cut: &str) -> Result<Option<f64>, String> {
        let (project, comp) = self.project()?;
        let composition = project.composition(&comp).map_err(|e| e.to_string())?;
        Ok(timebase::cut_to_comp(&project, composition, cut))
    }

    /// Story の Arrange。話題 `topics` を、カットの ID を `cuts` にして sequence `sequence` の末尾に並べる。
    /// 並べ先は送る直前の最新のデータで決める（[`arrange_ops`]）ので、パネルの一覧が古くても sequence を二重に作らない。
    pub fn arrange(
        &self,
        sequence: Option<&str>,
        topics: &[String],
        cuts: &[String],
    ) -> Result<EditOutcome, String> {
        let doc = self.shared.doc().ok_or("No project open")?;
        let topics: Vec<TopicId> = topics.iter().map(Id::new).collect();
        let cuts: Vec<CutId> = cuts.iter().map(Id::new).collect();
        let ops = arrange_ops(&doc.project, &doc.comp, sequence, topics, cuts)?;
        edit::submit_based(&self.shared, ops, doc.version)
    }
}

/// Arrange の操作。並べ先は `sequence`、無ければ先頭の sequence（パネルの `arrangeTarget` と同じ決め方）で、`insertTopics` だけ。
/// プロジェクトに sequence が無ければ、Insert と同じく開いているコンポジション `comp` に sequence とそのレイヤーを作って並べる
/// （sequence の自動作成）。どちらも 1 つのトランザクションなので、取り消し 1 回で全部戻る。
fn arrange_ops(
    project: &Project,
    comp: &CompositionId,
    sequence: Option<&str>,
    topics: Vec<TopicId>,
    cuts: Vec<CutId>,
) -> Result<Vec<Op>, String> {
    let target = sequence
        .map(Id::new)
        .or_else(|| project.sequences.first().map(|s| s.id.clone()));
    if let Some(sequence) = target {
        let insert = InsertTopics {
            sequence,
            index: None,
            topics,
            cuts,
        };
        return Ok(vec![insert.into()]);
    }
    let comp = project.composition(comp).map_err(|e| e.to_string())?;
    let length = topics_length(project, &topics)?;
    let new = NewSequence::ids(project);
    let fill = InsertTopics {
        sequence: new.sequence.clone(),
        index: None,
        topics,
        cuts,
    };
    Ok(new.ops(comp, length, vec![fill.into()]))
}

/// 話題の素材の範囲の長さの和。`insertTopics` は話題ごとに等速のカットを作るので、並べた sequence の長さになる。
fn topics_length(project: &Project, topics: &[TopicId]) -> Result<RationalTime, String> {
    if topics.is_empty() {
        return Err("No topics to arrange".into());
    }
    topics.iter().try_fold(RationalTime::ZERO, |sum, id| {
        let topic = project.topic(id).map_err(|e| e.to_string())?;
        let len = topic.out_point.checked_sub(topic.in_point);
        len.and_then(|l| sum.checked_add(l))
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests;
