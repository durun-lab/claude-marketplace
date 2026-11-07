//! 字幕を焼き込んだプロジェクトのデータの使い回しの判定。

use std::sync::Arc;

use mm_doc::Project;

/// 焼き込んだ結果と、その元になったもの。元のプロジェクトのデータと文字起こしのバージョンが同じ間だけ使う。
/// 単語の修正はプロジェクトのデータ（`transcript_edits`）に入るが、文字起こしの取り込みは入らないので両方を見る。
pub(super) struct Burned {
    pub from: Arc<Project>,
    /// `Core::transcript_version`。`Core` の無い JSON のプロジェクトは `None`（文字起こしを書く道が無い）。
    pub transcripts: Option<u64>,
    pub shown: Arc<Project>,
}

impl Burned {
    /// 今の `project` と文字起こしのバージョンから作った結果なら真。
    pub(super) fn fits(&self, project: &Arc<Project>, transcripts: Option<u64>) -> bool {
        Arc::ptr_eq(&self.from, project) && self.transcripts == transcripts
    }
}
