//! モバイルの提案（`/m/api/proposals`）。形は Claude の待ち受けの `/proposals` と同じ。
//!
//! 提案を出すのは「PC で決める」の下書きで、即時に適用できない操作も出せる（ロックの付いた所は断る）。
//! 承認・却下・保留に戻すは人の操作として受ける（Claude の MCP からは今のまま 403）。

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::devices::DeviceInfo;
use super::ops::MobileOpsBody;
use super::spans::{view_json, with_comp_spans};
use super::{MobileError, MobileState};
use crate::core::ServeError;
use crate::http::blocking;

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m/api/proposals", get(list).post(create))
        .route("/m/api/proposals/{id}", get(one))
        .route("/m/api/proposals/{id}/approve", post(approve))
        .route("/m/api/proposals/{id}/reject", post(reject))
        .route("/m/api/proposals/{id}/reopen", post(reopen))
}

/// 提案の応答には、範囲のコンポジションの時刻 `comp_spans` を足す（`spans`）。
async fn list(State(st): State<Arc<MobileState>>) -> Result<Response, MobileError> {
    let core = st.core.clone();
    let proposals = blocking(move || {
        let project = core.project()?.project;
        core.proposals()?
            .iter()
            .map(|v| with_comp_spans(&project, v))
            .collect::<Result<Vec<_>, _>>()
    })
    .await?;
    Ok(Json(json!({ "proposals": proposals })).into_response())
}

async fn one(
    State(st): State<Arc<MobileState>>,
    Path(id): Path<i64>,
) -> Result<Response, MobileError> {
    let core = st.core.clone();
    let view = blocking(move || view_json(&core, &core.proposal(id)?)).await?;
    Ok(Json(view).into_response())
}

/// `POST /m/api/ops` と同じ本体を、適用せずに保留する。
async fn create(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
    body: Bytes,
) -> Result<Response, MobileError> {
    let txn = MobileOpsBody::parse(&body)?;
    let caller = device.caller();
    let core = st.core.clone();
    let view = blocking(move || view_json(&core, &core.propose(txn, &caller, None)?)).await?;
    Ok(Json(view).into_response())
}

/// 承認の本体は任意。`ops` は承認する操作の番号（省くと全部）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApproveBody {
    ops: Option<Vec<usize>>,
}

async fn approve(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
    Path(id): Path<i64>,
    body: Bytes,
) -> Result<Response, MobileError> {
    let parsed: ApproveBody = if body.is_empty() {
        ApproveBody::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| ServeError::BadRequest(format!("本体の形が違う: {e}")))?
    };
    let caller = device.caller();
    let core = st.core.clone();
    let approval = blocking(move || {
        let a = core.approve(id, parsed.ops.as_deref(), &caller)?;
        Ok::<_, ServeError>(
            json!({ "proposal": view_json(&core, &a.proposal)?, "change": a.change }),
        )
    })
    .await?;
    Ok(Json(approval).into_response())
}

/// 却下と保留に戻すは提案の全体に掛かるので、本体を受けない。
fn require_empty(body: &Bytes) -> Result<(), ServeError> {
    let empty = body.is_empty() || body.as_ref() == b"{}";
    empty
        .then_some(())
        .ok_or_else(|| ServeError::BadRequest("却下と保留に戻すは本体を受けない".into()))
}

async fn reject(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
    Path(id): Path<i64>,
    body: Bytes,
) -> Result<Response, MobileError> {
    require_empty(&body)?;
    let caller = device.caller();
    let core = st.core.clone();
    let view = blocking(move || view_json(&core, &core.reject(id, &caller)?)).await?;
    Ok(Json(view).into_response())
}

async fn reopen(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
    Path(id): Path<i64>,
    body: Bytes,
) -> Result<Response, MobileError> {
    require_empty(&body)?;
    let caller = device.caller();
    let core = st.core.clone();
    let view = blocking(move || view_json(&core, &core.reopen(id, &caller)?)).await?;
    Ok(Json(view).into_response())
}
