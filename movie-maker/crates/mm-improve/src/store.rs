//! 要望の保存。1 件 1 ファイルで、未対応は `feedback/`、済み（取り込んだか PR までで止めたもの）は `feedback/done/` に置く。
//! ファイル名は時刻から始まるので、名前順が古い順になる。回の `claude -p` は、このファイルを読んで要望の全体を知る。

use std::path::{Path, PathBuf};

use mm_types::{MmError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::files::{read_json, write_json};
use crate::time::{now_ms, short_hex, stamp};

/// 要望の本文の上限（文字）。画面の状態などの大きいものは `context` に入れる。
pub const MAX_TEXT_CHARS: usize = 10_000;

/// 要望の出どころ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackKind {
    /// 人が Feedback パネルか CLI から送ったもの。
    Feedback,
    /// panic、例外、ホストのクラッシュから自動で起票したもの。
    Error,
    /// 予算を超えた遅い処理から自動で起票したもの。
    Slow,
}

/// 要望の状態。回が動いている間の「対応中」は、回の `items` から分かるので持たない。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackStatus {
    /// 未対応。次の回が拾う。
    #[default]
    Pending,
    /// 回が直して本線に取り込んだか、直さなくてよいと返事した。
    Done,
    /// 回が直したが、人の承認が要る領域に触れたので PR までで止めた。
    Held,
}

/// 回が要望に付けた結末。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    /// 本線に取り込んだ。
    Landed {
        /// 取り込んだコミット。
        commit: String,
    },
    /// コードを変えずに返事だけした。
    NoChange,
    /// PR までで止めた。
    PullRequest {
        /// PR の URL。
        url: String,
        /// 止めた理由。
        reason: String,
    },
    /// 囲いの検査で落ちたので取り込まなかった。要望は未対応に残る。
    Rejected {
        /// 落ちた理由。
        reason: String,
    },
}

/// 回からの返事。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// 返事を書いた時刻（ミリ秒）。
    pub at: u64,
    /// 回の ID。
    pub run_id: String,
    /// 回が書いた、何をしたか。
    pub text: String,
    /// 結末。
    pub outcome: Outcome,
}

/// 保存した要望。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackItem {
    /// ID。ファイル名から `.json` を除いたもの。
    pub id: String,
    /// 起票した時刻（ミリ秒）。
    pub created_at: u64,
    /// 出どころ。
    pub kind: FeedbackKind,
    /// 要望の本文。
    pub text: String,
    /// 人が画面で指した要素（Feedback パネルが決める形）。
    #[serde(default)]
    pub targets: Vec<Value>,
    /// 未対応か済みか。
    #[serde(default)]
    pub status: FeedbackStatus,
    /// 回からの返事。新しいものが後ろ。
    #[serde(default)]
    pub replies: Vec<Reply>,
    /// 送ったときの状態（画面、プロジェクト、自動の起票の記録など）。
    /// 大きいので最後に置き、ファイルの先頭で本文と返事を読めるようにする。
    #[serde(default)]
    pub context: Value,
}

/// 起票する要望。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewFeedback {
    /// 要望の本文。
    pub text: String,
    /// 人が画面で指した要素。
    pub targets: Vec<Value>,
    /// 送ったときの状態。
    pub context: Value,
}

/// `feedback/` の読み書き。
#[derive(Clone, Debug)]
pub struct FeedbackStore {
    dir: PathBuf,
}

impl FeedbackStore {
    /// `dir`（`<data_dir>/feedback`）を扱う。
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// 未対応の要望のフォルダー。回の `claude -p` に `--add-dir` で読ませる。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn done_dir(&self) -> PathBuf {
        self.dir.join("done")
    }

    /// 要望のファイルの場所（状態に応じて `feedback/` か `feedback/done/`）。
    pub fn path_of(&self, item: &FeedbackItem) -> PathBuf {
        let dir = match item.status {
            FeedbackStatus::Pending => self.dir.clone(),
            _ => self.done_dir(),
        };
        dir.join(format!("{}.json", item.id))
    }

    /// 起票して保存する。
    pub fn add(&self, kind: FeedbackKind, new: NewFeedback) -> Result<FeedbackItem> {
        let text = new.text.trim();
        if text.is_empty() {
            return Err(MmError::InvalidArgument("要望の本文が空".into()));
        }
        let created_at = now_ms();
        let item = FeedbackItem {
            id: format!("{}-{}", stamp(created_at), short_hex()),
            created_at,
            kind,
            text: text.chars().take(MAX_TEXT_CHARS).collect(),
            targets: new.targets,
            status: FeedbackStatus::Pending,
            replies: Vec::new(),
            context: new.context,
        };
        write_json(&self.path_of(&item), &item)?;
        Ok(item)
    }

    fn read_dir(dir: &Path) -> Result<Vec<FeedbackItem>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        let mut out = Vec::new();
        for p in paths {
            out.extend(read_json::<FeedbackItem>(&p)?);
        }
        Ok(out)
    }

    /// 未対応の要望（古い順）。
    pub fn pending(&self) -> Result<Vec<FeedbackItem>> {
        Self::read_dir(&self.dir)
    }

    /// 全部の要望（新しい順）。
    pub fn all(&self) -> Result<Vec<FeedbackItem>> {
        let mut v = Self::read_dir(&self.dir)?;
        v.extend(Self::read_dir(&self.done_dir())?);
        v.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(v)
    }

    /// ID で引く。
    pub fn get(&self, id: &str) -> Result<FeedbackItem> {
        check_id(id)?;
        let file = format!("{id}.json");
        for dir in [self.dir.clone(), self.done_dir()] {
            if let Some(item) = read_json(&dir.join(&file))? {
                return Ok(item);
            }
        }
        Err(MmError::NotFound(format!("要望 {id}")))
    }

    /// 書き直す。状態が変わったら、ファイルを `feedback/` と `done/` の間で移す。
    pub fn put(&self, item: &FeedbackItem) -> Result<()> {
        check_id(&item.id)?;
        let to = self.path_of(item);
        write_json(&to, item)?;
        let file = format!("{}.json", item.id);
        for dir in [self.dir.clone(), self.done_dir()] {
            let p = dir.join(&file);
            if p != to && p.exists() {
                std::fs::remove_file(p)?;
            }
        }
        Ok(())
    }
}

/// ID がフォルダーの外を指さないか確かめる。
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(MmError::InvalidArgument(format!("要望の ID が不正: {id}")))
    }
}
