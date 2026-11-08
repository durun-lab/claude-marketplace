//! 提案の区間の短い動画（`GET /m/api/proposals/{id}/clip/{key}.mp4`）。
//!
//! 提案を適用したプロジェクトを `mm-stream` の `Streamer::clip` に渡し、MP4 を `Range` に合わせて返す。
//! URL のキーは範囲に掛かるもののハッシュ（HLS の区間と同じ）で、提案の応答（`GET /m/api/proposals/{id}`）の `clip_url` が今のキーを渡す。
//! 作り終えた動画はキーで引くので、範囲の中が編集された後も再生の途中の `Range` には前と同じバイトで答える。
//! 作っていない古いキーは 404（端末は提案を読み直す）。今のキーへリダイレクトすると、続きの `Range` が別の MP4 のバイトになる。
//! 作るのは 6 秒の範囲で 1 秒足らずなので、202 で再試行させず、作り終えるまで要求を待たせる（HLS の区間と同じ）。
//! 同時に作る数の上限を超えた要求と、待つ上限を超えた要求は 503 で返し、描画のスレッドに要求を積み上げない。

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use mm_doc::Project;
use mm_store::ProposalState;
use mm_stream::{ClipKey, ClipRequest, Streamer};
use serde::Deserialize;

use super::guard::refuse;
use super::media::{matches_etag, mm_error, not_modified, respond, run, streamer};
use super::spans::clip_span;
use super::{MobileError, MobileState};
use crate::core::{Core, ServeError};
use crate::http::blocking;
use crate::proposals_http::parse_indices;

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new().route("/m/api/proposals/{id}/clip/{file}", get(clip))
}

/// 短い動画を選ぶ問い合わせ。提案の応答（`GET /m/api/proposals/{id}`）と短い動画の URL で同じ形。
#[derive(Debug, Deserialize)]
pub(super) struct ClipQuery {
    /// 範囲を見るコンポジション。省くと提案の範囲の最初のコンポジション。
    comp: Option<String>,
    /// 提案のうち適用する操作の番号（`0,2`）。省くと全部。
    ops: Option<String>,
}

/// 提案の区間の短い動画。保留中の提案は、そのキーの短い動画を作り終えていればそのまま返す。
/// 無ければ今のキーと比べ、同じなら作り、違えば 404。
/// 保留中でない提案は `GET /m/api/frame?proposal=` と同じく操作の層の拒否、範囲を持たない提案は 404 `no_range`。
async fn clip(
    State(st): State<Arc<MobileState>>,
    headers: HeaderMap,
    Path((id, file)): Path<(i64, String)>,
    Query(q): Query<ClipQuery>,
) -> Result<Response, MobileError> {
    let key: ClipKey = file.parse().map_err(mm_error)?;
    let streamer = streamer(&st)?;
    let selection = q.ops.as_deref().map(parse_indices).transpose()?;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let core = st.core.clone();
    let pending = blocking(move || Ok::<_, ServeError>(is_pending(&core, id))).await?;
    let cached = if pending {
        let (streamer, range) = (streamer.clone(), range.clone());
        run(move || streamer.cached_clip(key, range.as_deref())).await?
    } else {
        None
    };
    let payload = match cached {
        Some(payload) => payload,
        None => {
            let core = st.core.clone();
            let prepared =
                blocking(move || prepare(&core, id, selection.as_deref(), q.comp.as_deref()))
                    .await?;
            let (project, version, request) = prepared.ok_or_else(|| no_range(id))?;
            run(move || streamer.clip(&project, version, &request, key, range.as_deref())).await?
        }
    };
    if matches_etag(&headers, &payload) {
        return Ok(not_modified(&payload));
    }
    Ok(respond(payload))
}

/// 今の中身の短い動画の URL（`/m/api/proposals/{id}/clip/{key}.mp4?comp=&ops=`）。
/// 保留中でない提案、今のプロジェクトに当てられない提案、範囲を持たない提案、配信を持たない待ち受けは `None`。
pub(super) fn clip_url(
    core: &Core,
    streamer: Option<&Streamer>,
    state: ProposalState,
    id: i64,
    q: &ClipQuery,
) -> Result<Option<String>, ServeError> {
    let Some(streamer) = streamer.filter(|_| state == ProposalState::Pending) else {
        return Ok(None);
    };
    let selection = q.ops.as_deref().map(parse_indices).transpose()?;
    let prepared = match prepare(core, id, selection.as_deref(), q.comp.as_deref()) {
        // 後の編集で今のプロジェクトに当てられない提案も、詳細は返して短い動画だけを出さない。
        Err(ServeError::Op(_)) => return Ok(None),
        other => other?,
    };
    let Some((project, version, request)) = prepared else {
        return Ok(None);
    };
    let key = streamer
        .clip_key(&project, version, &request)
        .map_err(|e| ServeError::Internal(format!("短い動画のキーを作れない: {e}")))?;
    let mut url = format!(
        "/m/api/proposals/{id}/clip/{}?comp={}",
        key.file_name(),
        query_value(&request.composition)
    );
    if let Some(ops) = selection {
        let list: Vec<String> = ops.iter().map(usize::to_string).collect();
        let _ = write!(url, "&ops={}", list.join(","));
    }
    Ok(Some(url))
}

/// 問い合わせの値の文字のうち、予約されない文字（RFC 3986 の unreserved）以外を `%XX` にする。
fn query_value(raw: &str) -> String {
    raw.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
        out
    })
}

/// 保留中の提案か。無い提案と決めた提案は作り終えた動画も返さず、`prepare` の拒否に回す。
fn is_pending(core: &Core, id: i64) -> bool {
    core.proposal(id)
        .is_ok_and(|v| v.state == ProposalState::Pending)
}

fn no_range(id: i64) -> MobileError {
    refuse(
        StatusCode::NOT_FOUND,
        "no_range",
        &format!("提案 {id} はコンポジションの上の範囲を持たない"),
    )
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

#[cfg(test)]
mod tests {
    use super::query_value;

    #[test]
    fn 問い合わせの値は予約されない文字だけを残す() {
        assert_eq!(query_value("main-1_a.b~"), "main-1_a.b~");
        assert_eq!(query_value("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(query_value("縦"), "%E7%B8%A6");
    }
}
