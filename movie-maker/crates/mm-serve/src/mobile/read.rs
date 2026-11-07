//! モバイルの読み出し。`GET /m/api/project`（素材のパスを返さない要約）と `GET /m/api/events`（SSE）。

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use super::{MobileError, MobileState};
use crate::http::{blocking, event_stream};

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m/api/events", get(events))
        .route("/m/api/project", get(project))
}

/// `{ version, compositions, sequences, open_composition }`。素材の一覧（パスを持つ）は返さない。
/// `open_composition` は PC で開いているものを `Core` が知らないので、先頭のコンポジション。
async fn project(State(st): State<Arc<MobileState>>) -> Result<Response, MobileError> {
    let core = st.core.clone();
    let view = blocking(move || core.project()).await?;
    let p = &view.project;
    let open = p.compositions.first().map(|c| c.id.clone());
    Ok(Json(json!({
        "version": view.version,
        "compositions": p.compositions,
        "sequences": p.sequences,
        "open_composition": open,
    }))
    .into_response())
}

/// Claude の待ち受けの `GET /events` と同じ通知（`hello`、`change`、`resync`、`proposal`）。
async fn events(State(st): State<Arc<MobileState>>) -> Result<Response, MobileError> {
    Ok(event_stream(&st.core)?.into_response())
}
