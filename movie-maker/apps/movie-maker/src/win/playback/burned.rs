//! 字幕を焼き込んだプロジェクトのデータの使い回しの判定。

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use mm_doc::Project;
use mm_store::DB_FILE;

/// 文字起こしを書いた印。DB と WAL の更新時刻と大きさ。文字起こしの取り込みは操作ではなく変更の通知が出ないので、
/// プロジェクトのデータが同じでも単語が変わる。毎フレーム単語を読み直さず、ファイルの印で変化を見つける。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StoreStamp([Option<(SystemTime, u64)>; 2]);

impl StoreStamp {
    /// `dir` の `project.mmdb` と `-wal` の印。
    pub(super) fn of(dir: &Path) -> Self {
        let stat = |name: String| {
            let meta = std::fs::metadata(dir.join(name)).ok()?;
            Some((meta.modified().ok()?, meta.len()))
        };
        Self([stat(DB_FILE.to_owned()), stat(format!("{DB_FILE}-wal"))])
    }
}

/// 焼き込んだ結果と、その元になったもの。元のプロジェクトのデータと文字起こしの印が同じ間だけ使う。
pub(super) struct Burned {
    pub from: Arc<Project>,
    pub stamp: Option<StoreStamp>,
    pub shown: Arc<Project>,
}

impl Burned {
    /// 今の `project` と `stamp` から作った結果なら真。
    pub(super) fn fits(&self, project: &Arc<Project>, stamp: &Option<StoreStamp>) -> bool {
        Arc::ptr_eq(&self.from, project) && &self.stamp == stamp
    }
}
