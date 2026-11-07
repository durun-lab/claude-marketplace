//! 端末の登録の画面の裏側。人の待ち受け（`MobileServer`）をアプリと一緒に始め、登録の符号と端末の一覧を渡す。
//! 待ち受けの起動は tailnet の名前を CLI で読むので別のスレッドで行い、画面は `Starting` を見て読み直す。

use std::sync::{Arc, Mutex, MutexGuard};

use mm_serve::{
    serve_is_configured, DeviceInfo, MobileAsset, MobileAssets, MobileConfig, MobileServer,
    PairingCode, ServeError, MOBILE_HTTPS_PORT,
};
use serde::Serialize;

use crate::win::state::Shared;

/// ui のビルドが PWA を置く、アプリの画面の下のフォルダー（`ui/tools/build.ts`）。
const PWA_DIR: &str = "m";
/// 置き場に PWA があるかを確かめる、存在しない名前。
const MISSING_PROBE: &str = "__mm_missing__";
/// 設定の手順（12-mobile.md）で使う `tailscale serve` のコマンド。
const SERVE_COMMAND: &str = "tailscale serve --bg --https=8443 http://127.0.0.1:7338";

/// 画面に出す待ち受けの状態。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    /// 待ち受けを始めている途中。
    Starting,
    /// 待ち受けを始められなかった。`message` をそのまま画面に出す。
    Unavailable { message: String },
    /// 待ち受けている。`notices` が空でなければ、スマホから届くまでに足りないことがある。
    #[serde(rename_all = "camelCase")]
    Running {
        port: u16,
        /// モバイル版の URL。tailnet の名前が分からなければ `None`。
        url: Option<String>,
        serve_command: &'static str,
        notices: Vec<Notice>,
    },
}

/// スマホから届くまでに足りないこと。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    NoTailscale,
    ServeNotConfigured,
    NoPwa,
}

/// 登録の符号と、それを入れた登録の画面の QR。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub code: String,
    pub expires_at_ms: i64,
    /// スマホで開く URL。tailnet の名前が分からなければ `None`（符号を手で入れる）。
    pub url: Option<String>,
    pub qr_svg: Option<String>,
}

enum Launch {
    Starting,
    Failed(String),
    Running { server: MobileServer, has_pwa: bool },
}

/// Tauri の管理下に置く、端末の登録の画面の状態。
pub struct DevicesState {
    shared: Arc<Shared>,
    launch: Mutex<Launch>,
}

impl DevicesState {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            launch: Mutex::new(Launch::Starting),
        }
    }

    /// 人の待ち受けを別のスレッドで始める。失敗は `Status::Unavailable` として画面に出す。
    pub fn start(self: &Arc<Self>, handle: tauri::AppHandle) {
        let state = self.clone();
        let spawned = std::thread::Builder::new()
            .name("mm-mobile-start".into())
            .spawn(move || {
                let launch = state.launch_server(&handle);
                *state.lock() = launch;
            });
        if let Err(e) = spawned {
            *self.lock() = Launch::Failed(format!("The phone listener could not start: {e}"));
        }
    }

    fn launch_server(&self, handle: &tauri::AppHandle) -> Launch {
        let Some(core) = self.shared.core.get().cloned() else {
            return Launch::Failed(NO_PROJECT_STORE.into());
        };
        let mut config = match MobileConfig::detect() {
            Ok(c) => c,
            Err(e) => return Launch::Failed(failure_message(&e, 0)),
        };
        let port = config.port;
        let assets = TauriAssets::new(handle.clone());
        let has_pwa = assets.has_pwa();
        config.assets = has_pwa.then(|| Arc::new(assets) as Arc<dyn MobileAssets>);
        match MobileServer::start(core, config) {
            Ok(server) => {
                eprintln!("人の待ち受け: 127.0.0.1:{}", server.port());
                Launch::Running { server, has_pwa }
            }
            Err(e) => {
                tracing::warn!("人の待ち受けを起動できない: {e}");
                Launch::Failed(failure_message(&e, port))
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Launch> {
        self.launch.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 待ち受けの状態。`tailscale serve` の設定を CLI に聞くので、画面の読み直しのときだけ呼ぶ。
    pub fn status(&self) -> Status {
        let (port, url, has_pwa) = match &*self.lock() {
            Launch::Starting => return Status::Starting,
            Launch::Failed(message) => {
                return Status::Unavailable {
                    message: message.clone(),
                }
            }
            Launch::Running { server, has_pwa } => (server.port(), server.url(), *has_pwa),
        };
        // 画面の設定の確認（外部のコマンド）の間は、登録の操作を待たせない。
        let serve_ok = url.is_some() && serve_is_configured(MOBILE_HTTPS_PORT, port);
        Status::Running {
            port,
            notices: notices(url.is_some(), serve_ok, has_pwa, port),
            url,
            serve_command: SERVE_COMMAND,
        }
    }

    /// 登録の符号を出す。前の符号は無効になる。
    pub fn issue_code(&self) -> Result<Pairing, String> {
        let launch = self.lock();
        let Launch::Running { server, .. } = &*launch else {
            return Err("The phone listener is not running.".into());
        };
        let code = server.devices().issue_code().map_err(|e| e.to_string())?;
        let url = server.pairing_url(&code);
        let qr_svg = url.as_deref().and_then(qr_svg);
        Ok(pairing(code, url, qr_svg))
    }

    /// 登録した端末（登録した順）。
    pub fn list(&self) -> Result<Vec<DeviceInfo>, String> {
        match &*self.lock() {
            Launch::Running { server, .. } => server.devices().list().map_err(|e| e.to_string()),
            _ => Ok(Vec::new()),
        }
    }

    /// 端末の登録を外す。無い ID なら `false`。
    pub fn revoke(&self, id: &str) -> Result<bool, String> {
        match &*self.lock() {
            Launch::Running { server, .. } => {
                server.devices().revoke(id).map_err(|e| e.to_string())
            }
            _ => Ok(false),
        }
    }
}

const NO_PROJECT_STORE: &str =
    "Phone access needs a project folder with project.mmdb. Open one and restart movie-maker.";

fn pairing(code: PairingCode, url: Option<String>, qr_svg: Option<String>) -> Pairing {
    Pairing {
        code: code.code,
        expires_at_ms: code.expires_at_ms,
        url,
        qr_svg,
    }
}

/// 待ち受けを始められなかった理由。ポートが使われているときは、ほかの起動を閉じる手順を添える。
fn failure_message(e: &ServeError, port: u16) -> String {
    match e {
        ServeError::Unavailable(_) => format!(
            "Port {port} is already in use, so the phone listener could not start. \
             Close the other movie-maker window or the program using the port, then restart movie-maker."
        ),
        other => format!("The phone listener could not start: {other}"),
    }
}

/// スマホから届くまでに足りないことの文言。Tailscale が無ければ `tailscale serve` の確認は意味がないので出さない。
fn notices(has_tailnet: bool, serve_ok: bool, has_pwa: bool, port: u16) -> Vec<Notice> {
    let mut out = Vec::new();
    if !has_tailnet {
        out.push(Notice {
            kind: NoticeKind::NoTailscale,
            message:
                "Tailscale is not running or not installed, so this PC's tailnet name is unknown. \
                      Install Tailscale and sign in, then restart movie-maker. \
                      Until then, a phone cannot reach this PC."
                    .into(),
        });
    } else if !serve_ok {
        out.push(Notice {
            kind: NoticeKind::ServeNotConfigured,
            message: format!(
                "tailscale serve is not pointing at this app (port {port}) yet. \
                 Run the command below once in a terminal (not as administrator), then check again."
            ),
        });
    }
    if !has_pwa {
        out.push(Notice {
            kind: NoticeKind::NoPwa,
            message: "The phone app files are missing from this build. Run pnpm build in movie-maker/ui and rebuild the app."
                .into(),
        });
    }
    out
}

/// URL の QR を SVG で作る。スキャンしやすいよう、背景は白、点は黒にして余白を付ける。
fn qr_svg(url: &str) -> Option<String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    // XML の宣言は HTML に埋めるときに要らない。
    svg.find("<svg").map(|i| svg[i..].to_owned())
}

/// Tauri が実行ファイルに埋め込んだ画面（`ui/dist/m/`）を PWA の置き場にする。
pub struct TauriAssets {
    handle: tauri::AppHandle,
    /// Tauri は見つからない名前に shell の index.html を返す。その中身を覚え、PWA のファイルと取り違えない。
    shell_index: Option<Vec<u8>>,
}

impl std::fmt::Debug for TauriAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TauriAssets")
    }
}

impl TauriAssets {
    pub fn new(handle: tauri::AppHandle) -> Self {
        let mut assets = Self {
            handle,
            shell_index: None,
        };
        assets.shell_index = assets.fetch(MISSING_PROBE).map(|a| a.bytes);
        assets
    }

    fn fetch(&self, path: &str) -> Option<tauri::Asset> {
        self.handle
            .asset_resolver()
            .get(format!("{PWA_DIR}/{path}"))
    }

    /// PWA のファイル。shell の index.html が返ったときは、そのファイルが無いということ。
    fn lookup(&self, path: &str) -> Option<Vec<u8>> {
        let bytes = self.fetch(path)?.bytes;
        (self.shell_index.as_deref() != Some(bytes.as_slice())).then_some(bytes)
    }

    /// 置き場に PWA の index.html があるか。無い構成（`pnpm build` の前など）では `GET /m/` を 404 にする。
    fn has_pwa(&self) -> bool {
        self.lookup("index.html").is_some()
    }
}

impl MobileAssets for TauriAssets {
    fn get(&self, path: &str) -> Option<MobileAsset> {
        Some(MobileAsset::new(self.lookup(path)?, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ポートが使われているときは使われていると伝える() {
        let msg = failure_message(&ServeError::Unavailable("x".into()), 7338);
        assert!(msg.starts_with("Port 7338 is already in use"), "{msg}");
        let other = failure_message(&ServeError::Internal("boom".into()), 7338);
        assert!(other.contains("boom"), "{other}");
    }

    #[test]
    fn tailscale_が無ければ_serve_の確認は出さず_serve_が未設定なら手順へ導く() {
        let kinds = |n: Vec<Notice>| n.into_iter().map(|n| n.kind).collect::<Vec<_>>();
        assert_eq!(
            kinds(notices(false, false, true, 7338)),
            [NoticeKind::NoTailscale]
        );
        assert_eq!(
            kinds(notices(true, false, true, 7338)),
            [NoticeKind::ServeNotConfigured]
        );
        assert!(notices(true, true, true, 7338).is_empty());
        assert_eq!(kinds(notices(true, true, false, 7338)), [NoticeKind::NoPwa]);
    }

    #[test]
    fn qr_は_svg_の本体だけを返す() {
        let svg = qr_svg("https://desk.tail1234.ts.net:8443/m/pair?code=ABCD2345").unwrap();
        assert!(svg.starts_with("<svg"), "{}", &svg[..40]);
        assert!(svg.contains("#ffffff") && svg.contains("#000000"));
    }
}
