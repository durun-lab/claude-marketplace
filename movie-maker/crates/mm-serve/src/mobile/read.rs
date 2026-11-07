//! モバイルの読み出し。`GET /m/api/project`（素材のパスを返さない要約）と `GET /m/api/events`（SSE）。

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;

use super::devices::{DeviceInfo, Devices};
use super::guard::refuse;
use super::{MobileError, MobileState};
use crate::http::{blocking, event_items};

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m/api/events", get(events))
        .route("/m/api/project", get(project))
}

/// `{ version, compositions, sequences, open_composition, cuts }`。素材の一覧（パスを持つ）は返さない。
/// `open_composition` は PC で開いているものを `Core` が知らないので、先頭のコンポジション。
/// `cuts` はその境の秒（スクラブの棒の目盛り）。
async fn project(State(st): State<Arc<MobileState>>) -> Result<Response, MobileError> {
    let core = st.core.clone();
    let view = blocking(move || core.project()).await?;
    let p = &view.project;
    let open = p.compositions.first();
    let cuts = open.map_or_else(Vec::new, |c| super::cuts::boundaries(p, c));
    Ok(Json(json!({
        "version": view.version,
        "compositions": p.compositions,
        "sequences": p.sequences,
        "open_composition": open.map(|c| c.id.clone()),
        "cuts": cuts,
    }))
    .into_response())
}

/// Claude の待ち受けの `GET /events` と同じ通知（`hello`、`change`、`resync`、`proposal`、`comment`）。
/// 端末の登録を外したら列を閉じ、`EventSource` の繋ぎ直しは守りが 401 で断る。
async fn events(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
) -> Result<Response, MobileError> {
    // 守りを通ってから見張り始めるまでに外された端末は、見張りを始めた後に確かめ直して拾う。
    let revocations = st.devices.subscribe_revocations();
    if !st.devices.is_paired(&device.id)? {
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            "unknown_device",
            "端末の資格が登録に無いか外された",
        ));
    }
    let devices = st.devices.clone();
    let id = device.id;
    // 通知は `Some`、外されたことは `None` にして 1 つの列に混ぜ、`None` で列を終える。
    let revoked = WatchStream::from_changes(revocations)
        .filter(move |_| !still_paired(&devices, &id))
        .map(|_| None);
    let stream = event_items(&st.core)?
        .map(Some)
        .merge(revoked)
        .take_while(Option::is_some)
        .filter_map(|item: Option<Result<Event, Infallible>>| item);
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// 確かめられないとき（ロックが壊れた）も外されたとみなし、列を閉じる。
fn still_paired(devices: &Devices, id: &str) -> bool {
    devices.is_paired(id).unwrap_or(false)
}
