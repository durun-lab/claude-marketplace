//! モバイルの映像の道。コマ（`GET /m/api/frame`）、HLS（`/m/api/stream`）、素材の音だけ（`/m/api/audio`）。
//!
//! 描くのは `mm-stream` の `Streamer`（`Core` の描画と共有する 1 つの `Engine`）。ここは URL の形と、
//! `MmError` から HTTP の状態への写しだけを持つ。

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use mm_stream::{
    AudioRequest, HlsQuality, HlsRequest, Payload, SegmentKey, StillRequest, Streamer,
    STILL_MAX_WIDTH, STILL_MIN_WIDTH,
};
use mm_types::MmError;
use serde::Deserialize;

use super::guard::refuse;
use super::{MobileError, MobileState};
use crate::core::ServeError;
use crate::frames::{parse_time, FrameRequest};
use crate::http::blocking;
use crate::proposals_http::parse_indices;

/// 幅を省いたときのコマの幅。スクラブのつまみから提案の比べまで足りる中間。
const DEFAULT_STILL_WIDTH: u32 = 480;
/// プレイリストの区間の URI の前置き。プレイリストからの相対パスで `/m/api/stream/{comp}/seg/` を指すので、
/// どんな名前のコンポジション（`seg` も）ともパスの形がぶつからない。
const SEGMENT_BASE: &str = "seg/";
const PLAYLIST_EXT: &str = ".m3u8";
/// 親のプレイリストの名前（拡張子を除く）。画質の名前（`360p` など）とはぶつからない。
const MASTER_NAME: &str = "index";
const AUDIO_EXT: &str = ".m4a";

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m/api/audio/{file}", get(audio))
        .route("/m/api/frame", get(frame))
        .route("/m/api/stream/{comp}/{file}", get(playlist))
        .route("/m/api/stream/{comp}/seg/{file}", get(segment))
}

#[derive(Debug, Deserialize)]
struct FrameQuery {
    /// コンポジションの時間軸での時刻（`1.5`、`3/2`）。
    t: String,
    comp: Option<String>,
    width: Option<u32>,
    /// 提案の ID。あれば提案を適用したコマ（書かない）。
    proposal: Option<i64>,
    /// 提案のうち適用する操作の番号（`0,2`）。
    ops: Option<String>,
}

/// コマ。保存したデータのコマは WebP（バージョンごとにキャッシュ）、提案を適用したコマは PNG（キャッシュしない）。
async fn frame(
    State(st): State<Arc<MobileState>>,
    headers: HeaderMap,
    Query(q): Query<FrameQuery>,
) -> Result<Response, MobileError> {
    let time = parse_time(&q.t).map_err(ServeError::from)?;
    let width = q
        .width
        .unwrap_or(DEFAULT_STILL_WIDTH)
        .clamp(STILL_MIN_WIDTH, STILL_MAX_WIDTH);
    let core = st.core.clone();
    if let Some(id) = q.proposal {
        let selection = q.ops.as_deref().map(parse_indices).transpose()?;
        let request = FrameRequest {
            composition: q.comp,
            time,
            width: Some(width),
        };
        let png =
            blocking(move || core.preview_frame_png(id, selection.as_deref(), &request)).await?;
        return Ok((
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            png,
        )
            .into_response());
    }
    let streamer = streamer(&st)?;
    let request = StillRequest {
        composition: q.comp,
        time,
        width,
    };
    let payload = run(move || {
        let (version, project) = core.render_project().map_err(into_mm)?;
        streamer.still(&project, version.unsigned_abs(), &request)
    })
    .await?;
    if matches_etag(&headers, &payload) {
        return Ok(not_modified(&payload));
    }
    Ok(respond(payload))
}

/// `GET /m/api/stream/{comp}/index.m3u8`（画質を選ぶ親）と `GET /m/api/stream/{comp}/{quality}.m3u8`。
/// 子の URI は親からの相対パス `{quality}.m3u8`、区間の URI は子からの相対パス `seg/{key}.ts`。
async fn playlist(
    State(st): State<Arc<MobileState>>,
    Path((comp, file)): Path<(String, String)>,
) -> Result<Response, MobileError> {
    let name = file
        .strip_suffix(PLAYLIST_EXT)
        .ok_or_else(|| not_found("プレイリストの名前は index.m3u8 か <画質>.m3u8"))?;
    if name == MASTER_NAME {
        return master(&st, comp).await;
    }
    let quality: HlsQuality = name.parse().map_err(mm_error)?;
    let streamer = streamer(&st)?;
    let core = st.core.clone();
    let request = HlsRequest {
        composition: comp,
        quality,
    };
    let payload = run(move || {
        let (version, project) = core.render_project().map_err(into_mm)?;
        streamer.playlist(&project, version.unsigned_abs(), &request, SEGMENT_BASE)
    })
    .await?;
    Ok(respond(payload))
}

/// 画質を選ぶ親のプレイリスト。
async fn master(st: &Arc<MobileState>, comp: String) -> Result<Response, MobileError> {
    let streamer = streamer(st)?;
    let core = st.core.clone();
    let payload = run(move || {
        let (_, project) = core.render_project().map_err(into_mm)?;
        streamer.master_playlist(&project, &comp, &|q| {
            format!("{}{PLAYLIST_EXT}", q.as_str())
        })
    })
    .await?;
    Ok(respond(payload))
}

/// `GET /m/api/stream/{comp}/seg/{key}.ts`。キーが今のプレイリストに無ければ 404（端末はプレイリストを読み直す）。
/// 区間はキーだけで決まり、コンポジションの間でも共有するので `{comp}` は読まない。
async fn segment(
    State(st): State<Arc<MobileState>>,
    Path((_comp, file)): Path<(String, String)>,
) -> Result<Response, MobileError> {
    let key: SegmentKey = file.parse().map_err(mm_error)?;
    let streamer = streamer(&st)?;
    let payload = run(move || streamer.segment(key)).await?;
    Ok(respond(payload))
}

/// `GET /m/api/audio/{asset}.m4a`。`Range` に 200、206、416 で答える。
async fn audio(
    State(st): State<Arc<MobileState>>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> Result<Response, MobileError> {
    let asset = file
        .strip_suffix(AUDIO_EXT)
        .ok_or_else(|| not_found("音の名前は <素材>.m4a"))?
        .to_owned();
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let streamer = streamer(&st)?;
    let core = st.core.clone();
    let payload = run(move || {
        let (_, project) = core.render_project().map_err(into_mm)?;
        streamer.audio(&project, &AudioRequest { asset }, range.as_deref())
    })
    .await?;
    Ok(respond(payload))
}

fn streamer(st: &MobileState) -> Result<Arc<Streamer>, MobileError> {
    st.core.streamer().ok_or_else(|| {
        MobileError::Serve(ServeError::Unavailable(
            "この待ち受けは映像の配信を持たない".into(),
        ))
    })
}

/// 描画は長いので、非同期の実行環境をふさがないよう別のスレッドで走らせる。
async fn run<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, MmError> + Send + 'static,
) -> Result<T, MobileError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| mm_error(MmError::Internal(format!("処理が途中で終わった: {e}"))))?
        .map_err(mm_error)
}

/// `Core` の失敗を `run` の中で返すための写し。状態は `mm_error` で決めるので、種類を保てば足りる。
fn into_mm(e: ServeError) -> MmError {
    match e {
        ServeError::BadRequest(m) => MmError::InvalidArgument(m),
        ServeError::Unavailable(m) => MmError::Unsupported(m),
        other => MmError::Internal(other.to_string()),
    }
}

/// 配信の失敗を HTTP の状態にする。見つからない 404、引数の誤り 400、ffmpeg が無いなど 503、それ以外 500。
fn mm_error(e: MmError) -> MobileError {
    let (status, code) = match &e {
        MmError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        MmError::InvalidArgument(_) => (StatusCode::BAD_REQUEST, "bad_request"),
        MmError::Unsupported(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    refuse(status, code, &e.to_string())
}

fn not_found(message: &str) -> MobileError {
    refuse(StatusCode::NOT_FOUND, "not_found", message)
}

/// `Payload` をそのまま HTTP の応答にする。
fn respond(payload: Payload) -> Response {
    let mut res = Response::new(Body::from(payload.body));
    *res.status_mut() = StatusCode::from_u16(payload.status).unwrap_or(StatusCode::OK);
    for (name, value) in payload.headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            res.headers_mut().append(n, v);
        }
    }
    res
}

/// `If-None-Match` が応答の `ETag` と同じか。
fn matches_etag(headers: &HeaderMap, payload: &Payload) -> bool {
    let sent = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());
    matches!((sent, payload.header("ETag")), (Some(a), Some(b)) if a == b)
}

fn not_modified(payload: &Payload) -> Response {
    let mut res = respond(Payload {
        status: StatusCode::NOT_MODIFIED.as_u16(),
        headers: payload.headers.clone(),
        body: Vec::new(),
    });
    res.headers_mut().remove(header::CONTENT_LENGTH);
    res
}
