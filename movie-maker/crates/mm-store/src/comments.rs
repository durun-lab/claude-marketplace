//! レビューのコメント。`comments` の表に 1 件 1 行（`json` が本体、`state` と `comp_id` は検索用に抜いたカラム）で持つ。
//!
//! 状態は人が `open`、`done`、`wontfix` に決める。`fresh` は Claude の対応（`responds_to` を持つトランザクションの
//! 適用か提案の承認）が書いたときにだけなり、そのトランザクションを `responded_txn` で指す。結びつけは対応を書く
//! SQLite のトランザクションの中でするので、対応だけ残ってコメントが `open` のままにはならない。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use mm_types::{MmError, RationalTime, Result};

use crate::records::{Actor, StoredTxn};
use crate::store::Store;
use crate::SqlExt;

/// コメントの ID の接頭辞。`c-1`、`c-2`、...。
const COMMENT_ID_PREFIX: &str = "c-";
/// 返信の ID の接頭辞。コメントの中で一意。
const REPLY_ID_PREFIX: &str = "r-";

/// コメントの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentState {
    /// 人が書いた。まだ対応していない。
    Open,
    /// Claude が対応した。人が確かめる。
    Fresh,
    /// 人が確かめて済み。
    Done,
    /// 人が対応しないと決めた。
    Wontfix,
}

impl CommentState {
    /// カラムと JSON に書く文字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Fresh => "fresh",
            Self::Done => "done",
            Self::Wontfix => "wontfix",
        }
    }
}

/// 描き込みの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    /// 指でなぞった線。点の並び。
    Stroke,
    /// 矩形。対角の 2 点。
    Rect,
    /// 1 点。
    Point,
}

/// 描き込み 1 つ。座標はフレームの左上を (0, 0)、右下を (1, 1) に正規化する。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub kind: ShapeKind,
    pub points: Vec<[f64; 2]>,
}

/// 範囲のコメントの始めと終わり（コンポジションの時間軸の秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRange {
    #[serde(rename = "in")]
    pub start: RationalTime,
    pub out: RationalTime,
}

/// 返信。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: String,
    pub author: Actor,
    pub text: String,
    pub created_ms: i64,
}

/// コメント 1 件。時刻 `at` か範囲 `range` のどちらか 1 つを持つ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub comp: String,
    /// 時刻のコメントの時刻。範囲のコメントは `null`（端末が `at` を必ず読めるよう省かない）。
    #[serde(default)]
    pub at: Option<RationalTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<CommentRange>,
    pub text: String,
    #[serde(default)]
    pub drawing: Vec<Shape>,
    pub author: Actor,
    /// 端末が付けた ID。再送しても同じコメントを二重に書かないために使う。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default)]
    pub replies: Vec<Reply>,
    pub state: CommentState,
    pub created_ms: i64,
    /// Claude の対応のトランザクション（`fresh` にした最後のもの）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responded_txn: Option<i64>,
}

impl Comment {
    /// このコメントが `[from, to]` と重なるか。時刻のコメントは点として、範囲のコメントは区間として見る。
    fn overlaps(&self, from: RationalTime, to: RationalTime) -> bool {
        match (&self.at, &self.range) {
            (Some(at), _) => *at >= from && *at <= to,
            (None, Some(r)) => r.start <= to && r.out >= from,
            (None, None) => false,
        }
    }
}

/// 新しいコメントの内容。ID、状態、時刻は `Store` が決める。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct NewComment {
    pub comp: String,
    /// 端末が付けた ID。同じ値のコメントがあれば書かずにそれを返す。
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub at: Option<RationalTime>,
    #[serde(default)]
    pub range: Option<CommentRange>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub drawing: Vec<Shape>,
}

/// 一覧の絞り込み。指定した条件を全部満たすものだけ返す。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommentFilter {
    pub state: Option<CommentState>,
    pub comp: Option<String>,
    /// この区間（両端を含む）と重なる時刻か範囲のコメント。
    pub overlaps: Option<(RationalTime, RationalTime)>,
}

fn invalid(msg: impl Into<String>) -> MmError {
    MmError::InvalidArgument(msg.into())
}

fn validate_shapes(shapes: &[Shape]) -> Result<()> {
    for s in shapes {
        let ok_len = match s.kind {
            ShapeKind::Point => s.points.len() == 1,
            ShapeKind::Rect => s.points.len() == 2,
            ShapeKind::Stroke => s.points.len() >= 2,
        };
        if !ok_len {
            return Err(invalid(format!(
                "描き込み {:?} の点の数が違う: {}",
                s.kind,
                s.points.len()
            )));
        }
        if s.points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(invalid("描き込みの座標は 0 から 1"));
        }
    }
    Ok(())
}

fn validate_new(new: &NewComment) -> Result<()> {
    match (&new.at, &new.range) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(invalid("at か range のどちらか 1 つを渡す"))
        }
        (None, Some(r)) if r.start > r.out => return Err(invalid("range の in が out より後")),
        _ => {}
    }
    if new.text.trim().is_empty() && new.drawing.is_empty() {
        return Err(invalid("本文か描き込みが要る"));
    }
    validate_shapes(&new.drawing)
}

fn next_number<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str) -> u64 {
    ids.filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
        .max()
        .map_or(1, |n| n + 1)
}

fn write(conn: &rusqlite::Connection, c: &Comment) -> Result<()> {
    conn.execute(
        "INSERT INTO comments(id, comp_id, state, json) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET comp_id = excluded.comp_id,
           state = excluded.state, json = excluded.json",
        params![c.id, c.comp, c.state.as_str(), serde_json::to_string(c)?],
    )
    .sql()?;
    Ok(())
}

fn read(conn: &rusqlite::Connection, id: &str) -> Result<Option<Comment>> {
    let text: Option<String> = conn
        .query_row("SELECT json FROM comments WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()
        .sql()?;
    text.map(|t| serde_json::from_str(&t).map_err(MmError::from))
        .transpose()
}

/// 対応のトランザクションを、`responds_to` のコメントに結びつける。`open` か `fresh` のコメントを `fresh` にし、
/// 済み（`done`、`wontfix`）のコメントと、コメントでない ID は変えない。
pub(crate) fn link_response(tx: &rusqlite::Transaction, stored: &StoredTxn) -> Result<()> {
    let Some(id) = stored.responds_to.as_deref() else {
        return Ok(());
    };
    let Some(mut c) = read(tx, id)? else {
        return Ok(());
    };
    if !matches!(c.state, CommentState::Open | CommentState::Fresh) {
        return Ok(());
    }
    c.state = CommentState::Fresh;
    c.responded_txn = Some(stored.id);
    write(tx, &c)?;
    Ok(())
}

impl Store {
    /// コメント 1 件。無ければ `NotFound`。
    pub fn comment(&self, id: &str) -> Result<Comment> {
        read(&self.conn, id)?.ok_or_else(|| MmError::NotFound(format!("comment {id}")))
    }

    /// 条件に合うコメントを、書いた順（新しいものが後）に返す。
    pub fn list_comments(&self, filter: &CommentFilter) -> Result<Vec<Comment>> {
        let mut all: Vec<Comment> = self
            .comments()?
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(MmError::from))
            .collect::<Result<_>>()?;
        all.retain(|c| {
            filter.state.is_none_or(|s| c.state == s)
                && filter.comp.as_ref().is_none_or(|k| &c.comp == k)
                && filter.overlaps.is_none_or(|(a, b)| c.overlaps(a, b))
        });
        all.sort_by_key(|c| {
            let n =
                c.id.strip_prefix(COMMENT_ID_PREFIX)
                    .and_then(|n| n.parse::<u64>().ok());
            (c.created_ms, n.unwrap_or(u64::MAX), c.id.clone())
        });
        Ok(all)
    }

    /// コメントを書く。状態は `open`。
    pub fn add_comment(&mut self, new: &NewComment, author: &Actor, at_ms: i64) -> Result<Comment> {
        self.require_writer()?;
        validate_new(new)?;
        if !self
            .project()
            .compositions
            .iter()
            .any(|c| c.id.as_str() == new.comp)
        {
            return Err(MmError::NotFound(format!("composition {}", new.comp)));
        }
        let ids = self.comments()?;
        if let Some(key) = &new.client_id {
            let same = ids.iter().find(|c| c["client_id"].as_str() == Some(key));
            if let Some(found) = same {
                return serde_json::from_value(found.clone()).map_err(MmError::from);
            }
        }
        let n = next_number(
            ids.iter().filter_map(|c| c["id"].as_str()),
            COMMENT_ID_PREFIX,
        );
        let comment = Comment {
            client_id: new.client_id.clone(),
            id: format!("{COMMENT_ID_PREFIX}{n}"),
            comp: new.comp.clone(),
            at: new.at,
            range: new.range,
            text: new.text.clone(),
            drawing: new.drawing.clone(),
            author: author.clone(),
            replies: Vec::new(),
            state: CommentState::Open,
            created_ms: at_ms,
            responded_txn: None,
        };
        write(&self.conn, &comment)?;
        Ok(comment)
    }

    /// 返信を足す。状態は変えない。
    pub fn reply_comment(
        &mut self,
        id: &str,
        author: &Actor,
        text: &str,
        at_ms: i64,
    ) -> Result<Comment> {
        self.require_writer()?;
        if text.trim().is_empty() {
            return Err(invalid("返信の本文が空"));
        }
        let mut c = self.comment(id)?;
        let n = next_number(c.replies.iter().map(|r| r.id.as_str()), REPLY_ID_PREFIX);
        c.replies.push(Reply {
            id: format!("{REPLY_ID_PREFIX}{n}"),
            author: author.clone(),
            text: text.to_owned(),
            created_ms: at_ms,
        });
        write(&self.conn, &c)?;
        Ok(c)
    }

    /// 人が状態を決める（`open`、`done`、`wontfix`）。`fresh` は Claude の対応だけがするので拒否する。
    pub fn set_comment_state(&mut self, id: &str, state: CommentState) -> Result<Comment> {
        self.require_writer()?;
        if state == CommentState::Fresh {
            return Err(invalid(
                "fresh は Claude の対応（responds_to つきの適用か提案の承認）でだけなる",
            ));
        }
        let mut c = self.comment(id)?;
        c.state = state;
        write(&self.conn, &c)?;
        Ok(c)
    }

    /// 描き込みを足す。
    pub fn attach_drawing(&mut self, id: &str, shapes: &[Shape]) -> Result<Comment> {
        self.require_writer()?;
        validate_shapes(shapes)?;
        let mut c = self.comment(id)?;
        c.drawing.extend_from_slice(shapes);
        write(&self.conn, &c)?;
        Ok(c)
    }
}
