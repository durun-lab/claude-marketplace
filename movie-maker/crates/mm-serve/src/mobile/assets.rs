//! `GET /m/` の下の PWA の静的な配信。`ui/mobile` のビルドの出力（画面、マニフェスト、Service Worker）を返す。
//! 出力の置き場所はアプリが決める（`MobileAssets`）ので、ここは名前を引いて型と見出しを付けるだけ。

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;

use super::guard::PAIR_PATH;
use super::MobileState;

/// 画面の入口。`/m/`、`/m/pair`、画面の中の経路（拡張子の無いパス）はこれを返す。
const INDEX: &str = "index.html";
/// `/m/api/` の下は資格の要る API で、未知のパスを画面にしない。
const API_PREFIX: &str = "api/";
/// Service Worker の名前。スコープを `/m/` に絞る見出しを付ける。
const SERVICE_WORKER: &str = "sw.js";

/// PWA の 1 ファイル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobileAsset {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

impl MobileAsset {
    /// 型は `path` の拡張子から決める（Service Worker とマニフェストは型が合わないと登録されない）。
    pub fn new(bytes: Vec<u8>, path: &str) -> Self {
        Self {
            bytes,
            mime: mime_for(path),
        }
    }
}

/// PWA のファイルの置き場。`path` は `/m/` からの相対（`app.js`、`index.html`）。無ければ `None`。
pub trait MobileAssets: std::fmt::Debug + Send + Sync {
    fn get(&self, path: &str) -> Option<MobileAsset>;
}

/// ディレクトリーから読む置き場（`ui/mobile/dist/` をそのまま指す開発と試験の用）。
#[derive(Debug, Clone)]
pub struct DirAssets {
    root: PathBuf,
}

impl DirAssets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl MobileAssets for DirAssets {
    fn get(&self, path: &str) -> Option<MobileAsset> {
        let rel = Path::new(path);
        // `..` や絶対パスで置き場の外を読ませない。
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return None;
        }
        let bytes = std::fs::read(self.root.join(rel)).ok()?;
        Some(MobileAsset::new(bytes, path))
    }
}

/// 拡張子から返す型。PWA のビルドが出す種類だけ。
pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        Some("json" | "map") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m", get(|| async { Redirect::permanent("/m/") }))
        .route("/m/", get(root))
        // 登録の POST と同じパス。GET は登録の画面（PWA の入口）を返す。
        .route(PAIR_PATH, get(root))
        // GET 以外は 405 でなく 404 にして、人の待ち受けに無い道（/m/mcp など）を同じ答えにする。
        .route("/m/{*path}", get(file).fallback(not_found))
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn root(State(st): State<Arc<MobileState>>) -> Response {
    serve(&st, INDEX)
}

async fn file(State(st): State<Arc<MobileState>>, UrlPath(path): UrlPath<String>) -> Response {
    serve(&st, &path)
}

/// 名前のファイルを返す。拡張子の無い名前は画面の中の経路なので入口を返し、拡張子のある名前は無ければ 404。
fn serve(st: &MobileState, path: &str) -> Response {
    let not_found = || StatusCode::NOT_FOUND.into_response();
    if path.starts_with(API_PREFIX) {
        return not_found();
    }
    let Some(assets) = &st.assets else {
        return not_found();
    };
    let is_route = !path.rsplit('/').next().unwrap_or_default().contains('.');
    let (name, asset) = match assets.get(path) {
        Some(a) => (path, a),
        None if is_route => match assets.get(INDEX) {
            Some(a) => (INDEX, a),
            None => return not_found(),
        },
        None => return not_found(),
    };
    let mut res = ([(header::CONTENT_TYPE, asset.mime)], asset.bytes).into_response();
    let h = res.headers_mut();
    // ファイル名にハッシュが無いので、版の更新が届くよう毎回確かめさせる（高速化は Service Worker の担当）。
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if name == SERVICE_WORKER {
        h.insert("service-worker-allowed", HeaderValue::from_static("/m/"));
    }
    res
}
