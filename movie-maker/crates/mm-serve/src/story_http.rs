//! Story の読み出しの HTTP。パスと形は `docs/design/12-mobile.md` の「HTTP の形」と
//! `ui/panels/story/src/api.ts` の `StoryApi` と同じ（`overview`、`transcript`、`range`、`search`）。
//!
//! 認証は付けない。人の待ち受けが [`story_routes`] を `/m/api` の下に載せ、自分の認証の層を掛ける。
//! 読み出しは `mm-story`。保存済みのプロジェクトのデータを読むので、アプリの未保存の修正は見えない。

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mm_doc::Project;
use mm_story::{Overview, StoryHit, StoryReader, StoryTranscript, WordRange};
use mm_types::{CompositionId, Id};
use serde::Deserialize;

use crate::core::{Core, ServeError};
use crate::http::blocking;

/// 検索結果の件数の既定。`mm search` と同じ。
const DEFAULT_SEARCH_LIMIT: usize = 50;
/// プロジェクトのフォルダーの下の、取り込みの付属ファイルの置き場。
const CACHE_DIR: &str = "cache";

/// `GET /story/overview` の問い合わせ。
#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    /// 開いているコンポジション。無ければ最初のコンポジション。
    pub comp: Option<String>,
}

/// `GET /story/transcript` の問い合わせ。
#[derive(Debug, Deserialize)]
pub struct TranscriptQuery {
    pub asset: String,
}

/// `GET /story/range` の問い合わせ。`first` と `last` はどちらが先でもよい。
#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub asset: String,
    pub first: String,
    pub last: String,
}

/// `GET /story/search` の問い合わせ。
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

/// Story の 4 つの読み出し。パスは `/story/...` で、呼び出し側が `nest` で `/m/api` の下に置く。
pub fn story_routes(core: Arc<Core>) -> Router {
    Router::new()
        .route("/story/overview", get(overview))
        .route("/story/transcript", get(transcript))
        .route("/story/range", get(range))
        .route("/story/search", get(search))
        .with_state(core)
}

/// 保存済みのプロジェクトのデータと、その文字起こしの置き場。`Store` のロックは読み出しの前に放す。
fn snapshot(core: &Core) -> Result<(Project, StoryReader), ServeError> {
    core.with_store(|s| {
        let dir: PathBuf = s.dir().to_path_buf();
        let reader = StoryReader::with_store_dir(Some(dir.clone()), dir.join(CACHE_DIR));
        Ok((s.project().clone(), reader))
    })
}

fn bad(message: String) -> ServeError {
    ServeError::BadRequest(message)
}

async fn overview(
    State(core): State<Arc<Core>>,
    Query(q): Query<OverviewQuery>,
) -> Result<Json<Overview>, ServeError> {
    let out = blocking(move || {
        let (project, reader) = snapshot(&core)?;
        let comp: CompositionId = q
            .comp
            .map(|c| Id::new(&c))
            .or_else(|| project.compositions.first().map(|c| c.id.clone()))
            .unwrap_or_else(|| Id::new(""));
        Ok(reader.overview(&project, &comp))
    })
    .await?;
    Ok(Json(out))
}

async fn transcript(
    State(core): State<Arc<Core>>,
    Query(q): Query<TranscriptQuery>,
) -> Result<Json<StoryTranscript>, ServeError> {
    let out = blocking(move || {
        let (project, reader) = snapshot(&core)?;
        reader.transcript(&project, &q.asset).map_err(bad)
    })
    .await?;
    Ok(Json(out))
}

async fn range(
    State(core): State<Arc<Core>>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<WordRange>, ServeError> {
    let out = blocking(move || {
        let (project, reader) = snapshot(&core)?;
        reader
            .range(&project, &q.asset, &q.first, &q.last)
            .map_err(bad)
    })
    .await?;
    Ok(Json(out))
}

async fn search(
    State(core): State<Arc<Core>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<StoryHit>>, ServeError> {
    let limit = q.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
    let out = blocking(move || {
        let (project, reader) = snapshot(&core)?;
        reader.search(&project, &q.q, limit).map_err(bad)
    })
    .await?;
    Ok(Json(out))
}
