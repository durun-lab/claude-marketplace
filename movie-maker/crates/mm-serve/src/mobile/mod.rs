//! 人の待ち受け（モバイル版）。`docs/design/12-mobile.md` の「つながり方」と「HTTP の形」。
//!
//! 127.0.0.1 の決まったポートで待ち、`tailscale serve` が HTTPS で tailnet に出す。Claude の待ち受けと
//! ルーターを分け、`/mcp` と `entry` の指定を持たない。ここから来た操作は全部 `Entry::Mobile`（人）で、
//! 名乗りは登録した端末の名前。パスは全部 `/m` の下に置く。

mod devices;
mod guard;
mod ops;
mod pair;
mod proposals;
mod read;
mod tailnet;

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::json;
use tokio::runtime::{Builder, Runtime};

use crate::core::{Caller, Core, ServeError};

pub use devices::{
    DeviceInfo, Devices, PairingCode, CREDENTIAL_BYTES, PAIRING_CODE_LEN, PAIRING_MAX_FAILURES,
    PAIRING_TTL,
};
pub use guard::{COOKIE_NAME, LOGIN_HEADER, MOBILE_HEADER, PAIR_PATH};
pub use ops::MOBILE_OPS;
pub use tailnet::{hosts_in_status, tailnet_hosts};

/// 人の待ち受けの既定のポート。
pub const MOBILE_PORT: u16 = 7338;
/// `tailscale serve --https=<port>` の既定のポート。durun-dashboard の 443 と別のオリジンにする。
pub const MOBILE_HTTPS_PORT: u16 = 8443;
/// 端末の一覧のファイルの名前（持ち主の、消えないアプリのフォルダーに置く）。
pub const DEVICES_FILE: &str = "devices.json";
/// 要求の本体の上限。操作とコメントが収まればよい。
const BODY_LIMIT: usize = 1024 * 1024;
/// 待ち受けの非同期の実行環境のスレッド数。
const WORKER_THREADS: usize = 2;

/// 人の待ち受けの設定。
#[derive(Debug, Clone)]
pub struct MobileConfig {
    /// 127.0.0.1 で待つポート。0 なら空いているポート（試験用）。
    pub port: u16,
    /// `Host` に許す tailnet での名前。空なら全部の要求を 403 にする。
    pub hosts: Vec<String>,
    /// `tailscale serve` の HTTPS のポート。モバイル版の URL に使う。
    pub https_port: u16,
    /// 端末の一覧（`devices.json`）のパス。
    pub devices_file: PathBuf,
    /// 登録の符号の期限。
    pub pairing_ttl: Duration,
}

impl MobileConfig {
    /// 既定のポートと期限で、名前と端末の一覧のパスだけを決めた設定。
    pub fn new(hosts: Vec<String>, devices_file: PathBuf) -> Self {
        Self {
            port: MOBILE_PORT,
            hosts,
            https_port: MOBILE_HTTPS_PORT,
            devices_file,
            pairing_ttl: PAIRING_TTL,
        }
    }

    /// `tailscale status` の名前と、既定の置き場所（`%LOCALAPPDATA%\movie-maker\devices.json`）の設定。
    pub fn detect() -> Result<Self, ServeError> {
        let file = mm_store::private_app_file(DEVICES_FILE)?;
        Ok(Self::new(tailnet_hosts(), file))
    }
}

/// 人の待ち受けのハンドラーが共有するもの。
pub(crate) struct MobileState {
    pub core: Arc<Core>,
    pub hosts: Vec<String>,
    pub devices: Arc<Devices>,
}

/// 人の待ち受けの失敗。`ServeError` と同じ `{"error": {"code","message", ...}}` で返す。
#[derive(Debug)]
pub(crate) enum MobileError {
    Serve(ServeError),
    /// 人の待ち受けだけの拒否（守り、`not_on_mobile`、`not_latest` など）。
    Refuse {
        status: StatusCode,
        code: &'static str,
        message: String,
    },
}

impl From<ServeError> for MobileError {
    fn from(e: ServeError) -> Self {
        Self::Serve(e)
    }
}

impl IntoResponse for MobileError {
    fn into_response(self) -> Response {
        match self {
            Self::Serve(e) => e.into_response(),
            Self::Refuse {
                status,
                code,
                message,
            } => (
                status,
                Json(json!({ "error": { "code": code, "message": message } })),
            )
                .into_response(),
        }
    }
}

impl DeviceInfo {
    /// この端末から来た操作の呼び出し側（`Entry::Mobile`、名乗りは端末の名前）。
    pub fn caller(&self) -> Caller {
        Caller::mobile(&self.name)
    }
}

/// 人の待ち受けのルーター。ほかのモジュールの `/m/api/` は名前の順に 1 行ずつ `merge` する。
pub(crate) fn router(state: Arc<MobileState>) -> Router {
    Router::new()
        .merge(ops::routes())
        .merge(pair::routes())
        .merge(proposals::routes())
        .merge(read::routes())
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(middleware::from_fn_with_state(state.clone(), guard::guard))
        .with_state(state)
}

/// 動いている人の待ち受け。落とすと待ち受けを止める。
pub struct MobileServer {
    port: u16,
    config: MobileConfig,
    devices: Arc<Devices>,
    runtime: Option<Runtime>,
}

impl MobileServer {
    /// 127.0.0.1 の `config.port` で待ち始める。ポートが使われていれば `ServeError::Unavailable`。
    pub fn start(core: Arc<Core>, config: MobileConfig) -> Result<Self, ServeError> {
        let devices = Arc::new(Devices::open(
            config.devices_file.clone(),
            config.pairing_ttl,
        )?);
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, config.port)))
            .map_err(|e| {
                ServeError::Unavailable(format!("ポート {} で待てない: {e}", config.port))
            })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| ServeError::Internal(e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| ServeError::Internal(e.to_string()))?
            .port();
        let runtime = Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .thread_name("mm-serve-mobile")
            .enable_all()
            .build()
            .map_err(|e| ServeError::Internal(format!("実行環境を作れない: {e}")))?;
        let state = Arc::new(MobileState {
            core,
            hosts: config.hosts.clone(),
            devices: devices.clone(),
        });
        let listener = {
            let _enter = runtime.enter();
            tokio::net::TcpListener::from_std(listener)
                .map_err(|e| ServeError::Internal(e.to_string()))?
        };
        runtime.spawn(async move {
            if let Err(e) = axum::serve(listener, router(state)).await {
                tracing::error!("人の待ち受けが止まった: {e}");
            }
        });
        Ok(Self {
            port,
            config,
            devices,
            runtime: Some(runtime),
        })
    }

    /// 待っているポート。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 登録した端末と登録の符号。PC のアプリが端末の登録の画面で使う。
    pub fn devices(&self) -> &Arc<Devices> {
        &self.devices
    }

    /// モバイル版の URL（`https://<tailnet での名前>:8443/m/`）。tailnet の名前が分からなければ `None`。
    pub fn url(&self) -> Option<String> {
        let host = self.config.hosts.first()?;
        Some(format!("https://{host}:{}/m/", self.config.https_port))
    }

    /// 登録の符号を入れた登録の画面の URL（`.../m/pair?code=`、QR に載せる）。
    pub fn pairing_url(&self, code: &PairingCode) -> Option<String> {
        Some(format!("{}pair?code={}", self.url()?, code.code))
    }
}

impl Drop for MobileServer {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}
