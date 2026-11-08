//! `POST /m/pair`。PC のアプリが出した登録コードと引き換えに、端末の資格をクッキーで渡す。
//! 資格は本文に入れない（画面のスクリプトから読めない `HttpOnly` のクッキーにだけ置く）。

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::devices::PairError;
use super::guard::{refuse, COOKIE_NAME, LOGIN_HEADER, PAIR_PATH};
use super::{MobileError, MobileState};
use crate::core::ServeError;
use crate::http::blocking;

/// クッキーの寿命（秒）。Chrome が許す上限の 400 日。過ぎたら登録し直す。
const COOKIE_MAX_AGE_SECS: u64 = 400 * 24 * 60 * 60;

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new().route(PAIR_PATH, post(pair))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairBody {
    code: String,
    device_name: String,
}

async fn pair(
    State(st): State<Arc<MobileState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, MobileError> {
    let parsed: PairBody = serde_json::from_slice(&body)
        .map_err(|e| ServeError::BadRequest(format!("本体の形が違う: {e}")))?;
    // ガードがヘッダーの有無を確かめた後なので、ここでは値を読むだけ。
    let login = headers
        .get(LOGIN_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let devices = st.devices.clone();
    let outcome = blocking(move || devices.pair(&parsed.code, &parsed.device_name, &login)).await?;
    let (device, credential) = outcome.map_err(|e| {
        tracing::warn!(reason = ?e, "端末の登録を断った");
        pair_refusal(e)
    })?;
    tracing::info!(device = device.name, "端末を登録した");
    let cookie = format!(
        "{COOKIE_NAME}={credential}; Path=/m; HttpOnly; Secure; SameSite=Strict; Max-Age={COOKIE_MAX_AGE_SECS}"
    );
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(json!({ "device_name": device.name, "device": device })),
    )
        .into_response())
}

fn pair_refusal(e: PairError) -> MobileError {
    match e {
        PairError::NoCode => refuse(
            StatusCode::UNAUTHORIZED,
            "pairing_unavailable",
            "登録コードが無いか期限が切れた（PC のアプリで出し直す）",
        ),
        PairError::WrongCode => refuse(
            StatusCode::UNAUTHORIZED,
            "pairing_wrong_code",
            "登録コードが違う",
        ),
        PairError::BadName => refuse(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "端末の名前が空か長すぎる",
        ),
        PairError::NameTaken => refuse(
            StatusCode::CONFLICT,
            "name_taken",
            "同じ名前の端末が登録されている",
        ),
    }
}
