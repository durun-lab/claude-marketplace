//! 提案の区間の短い動画（`GET /m/api/proposals/{id}/clip.mp4`）。
//!
//! 提案を適用したプロジェクトを `mm-stream` の `Streamer::clip` に渡し、MP4 を `Range` に合わせて返す。
//! 作るのは 6 秒の範囲で 1 秒足らずなので、202 で再試行させず、作り終えるまで要求を待たせる（HLS の区間と同じ）。
//! `<video>` は 202 を再試行せず、待たせれば押した操作の中の `play()` のまま再生が始まる。
//! 同時に作る数の上限を超えた要求と、待つ上限を超えた要求は 503 で返し、描画のスレッドに要求を積み上げない。

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use mm_doc::Project;
use mm_stream::ClipRequest;
use serde::Deserialize;

use super::guard::refuse;
use super::media::{matches_etag, not_modified, respond, run, streamer};
use super::spans::clip_span;
use super::{MobileError, MobileState};
use crate::core::{Core, ServeError};
use crate::http::blocking;
use crate::proposals_http::parse_indices;

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new().route("/m/api/proposals/{id}/clip.mp4", get(clip))
}

#[derive(Debug, Deserialize)]
struct ClipQuery {
    /// 範囲を見るコンポジション。省くと提案の範囲の最初のコンポジション。
    comp: Option<String>,
    /// 提案のうち適用する操作の番号（`0,2`）。省くと全部。
    ops: Option<String>,
}

/// 提案の区間の短い動画。保留中でない提案は `GET /m/api/frame?proposal=` と同じく操作の層の拒否、
/// 範囲を持たない提案は 404 `no_range`。
async fn clip(
    State(st): State<Arc<MobileState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Query(q): Query<ClipQuery>,
) -> Result<Response, MobileError> {
    let streamer = streamer(&st)?;
    let selection = q.ops.as_deref().map(parse_indices).transpose()?;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let core = st.core.clone();
    let prepared =
        blocking(move || prepare(&core, id, selection.as_deref(), q.comp.as_deref())).await?;
    let Some((project, version, request)) = prepared else {
        return Err(refuse(
            StatusCode::NOT_FOUND,
            "no_range",
            &format!("提案 {id} はコンポジションの上の範囲を持たない"),
        ));
    };
    let payload = run(move || streamer.clip(&project, version, &request, range.as_deref())).await?;
    if matches_etag(&headers, &payload) {
        return Ok(not_modified(&payload));
    }
    Ok(respond(payload))
}

/// 提案を適用したプロジェクト、今のバージョン、短い動画の要求。範囲が無ければ `None`。
fn prepare(
    core: &Core,
    id: i64,
    selection: Option<&[usize]>,
    comp: Option<&str>,
) -> Result<Option<(Project, u64, ClipRequest)>, ServeError> {
    // バージョンは適用より先に読む。間に編集が入っても、新しいバージョンのキーで古い中身をキャッシュに残さない。
    let saved = core.project()?;
    // 適用を先に試し、無い提案と保留中でない提案を `GET /m/api/frame?proposal=` と同じ拒否にする。
    let (applied, touched) = core.preview_project(id, selection)?;
    let view = core.proposal(id)?;
    let Some((composition, start, end)) = clip_span(&saved.project, &applied, &view.scope, comp)
    else {
        return Ok(None);
    };
    let ops: Vec<_> = view
        .ops
        .iter()
        .filter(|o| selection.is_none_or(|s| s.contains(&o.index)))
        .map(|o| &o.op)
        .collect();
    let ops = serde_json::to_string(&ops)
        .map_err(|e| ServeError::Internal(format!("提案の操作を JSON にできない: {e}")))?;
    let request = ClipRequest {
        composition,
        start,
        end,
        ops,
        touched,
    };
    Ok(Some((applied, saved.version.unsigned_abs(), request)))
}
