//! 人の待ち受け（モバイル版）の結合テスト。`tailscale serve` の代わりに、tailnet での名前の `Host` と
//! `Tailscale-User-Login` を自分で付けて送る。

mod guard;
mod ops;

use std::path::Path;
use std::time::Duration;

use mm_doc::Project;
use mm_serve::{Client, Core, MobileConfig, MobileServer, Response, Server};
use mm_store::Store;
use serde_json::{json, Value};

const NOW: i64 = 1_700_000_000_000;
/// tailnet での名前の代わり。
pub const HOST: &str = "desk.tail1234.ts.net";
pub const LOGIN: &str = "me@example.com";

pub struct Rig {
    pub tmp: tempfile::TempDir,
    pub claude: Client,
    pub mobile: MobileServer,
    _server: Server,
}

/// 段階 0 のフィクスチャ（ロックを付けられるレイヤー `l-text` がある）で、Claude と人の待ち受けを始める。
pub fn rig() -> Rig {
    rig_with(|_| {})
}

pub fn rig_with(tweak: impl FnOnce(&mut MobileConfig)) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/stage0/project.json");
    let project: Project = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let store = Store::create(&tmp.path().join("p"), &project, NOW).unwrap();
    let core = Core::new(store, None);
    let server = Server::start(core.clone()).unwrap();
    let claude = Client::new(server.port(), server.token());
    let mut config = MobileConfig::new(vec![HOST.into()], tmp.path().join("devices.json"));
    config.port = 0;
    tweak(&mut config);
    let mobile = MobileServer::start(core, config).unwrap();
    Rig {
        tmp,
        claude,
        mobile,
        _server: server,
    }
}

/// 人の待ち受けに送る見出し。`None` の見出しは付けない。
#[derive(Clone)]
pub struct Req {
    pub host: Option<String>,
    pub origin: Option<String>,
    pub login: Option<String>,
    pub cookie: Option<String>,
    pub mobile: bool,
}

impl Req {
    /// 登録していない端末の、`tailscale serve` を通った要求。
    pub fn tailnet() -> Self {
        Self {
            host: Some(format!("{HOST}:8443")),
            origin: Some(format!("https://{HOST}:8443")),
            login: Some(LOGIN.into()),
            cookie: None,
            mobile: true,
        }
    }

    /// 登録した端末の要求。
    pub fn device(cookie: &str) -> Self {
        Self {
            cookie: Some(cookie.into()),
            ..Self::tailnet()
        }
    }

    fn headers(&self) -> Vec<(String, String)> {
        let mut h = Vec::new();
        let mut put = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                h.push((k.to_owned(), v.clone()));
            }
        };
        put("Host", &self.host);
        put("Origin", &self.origin);
        put("Tailscale-User-Login", &self.login);
        put("Cookie", &self.cookie);
        if self.mobile {
            h.push(("X-MM-Mobile".into(), "1".into()));
        }
        h
    }
}

impl Rig {
    pub fn send(&self, req: &Req, method: &str, path: &str, body: Option<&Value>) -> Response {
        let client = Client::new(self.mobile.port(), "");
        let bytes = body.map(Value::to_string);
        client
            .send(
                method,
                path,
                &req.headers(),
                bytes.as_deref().map(str::as_bytes),
            )
            .unwrap()
    }

    /// 登録の符号を出して登録し、`Cookie` に入れる値（`mm_device=...`）を返す。
    pub fn pair(&self, name: &str) -> String {
        let code = self.mobile.devices().issue_code().unwrap().code;
        let res = self.send(
            &Req::tailnet(),
            "POST",
            "/m/pair",
            Some(&json!({"code": code, "device_name": name})),
        );
        assert_eq!(res.status, 200, "{}", res.error_message());
        let set = res.header("set-cookie").unwrap();
        set.split(';').next().unwrap().to_owned()
    }

    pub fn version(&self) -> i64 {
        self.claude.get("/project").unwrap().json().unwrap()["version"]
            .as_i64()
            .unwrap()
    }
}

pub fn code(res: &Response) -> String {
    res.json().unwrap()["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// 期限を 0 にした符号は、出した直後から使えない。
pub fn expired_rig() -> Rig {
    rig_with(|c| c.pairing_ttl = Duration::ZERO)
}
