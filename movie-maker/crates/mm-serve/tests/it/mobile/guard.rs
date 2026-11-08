//! 人の待ち受けのガードと端末の登録。どの検査で拒否されても、操作は適用されない。

use mm_serve::PAIRING_MAX_FAILURES;
use serde_json::{json, Value};

use super::{code, expired_rig, rig, Req, Rig, LOGIN};

fn marker(id: &str, secs: i64) -> Value {
    json!({
        "op": {"op": "addMarker", "composition": "main",
               "marker": {"id": id, "time": {"num": secs, "den": 1}, "name": "見る"}},
        "base_version": 0,
    })
}

fn post_marker(r: &Rig, req: &Req) -> mm_serve::Response {
    r.send(req, "POST", "/m/api/ops", Some(&marker("m1", 1)))
}

#[test]
fn 各検査で拒否された要求は適用されない() {
    let r = rig();
    let cookie = r.pair("phone");
    let ok = Req::device(&cookie);
    let cases = [
        (
            "tailnet の名前でない Host",
            Req {
                host: Some("evil.example".into()),
                ..ok.clone()
            },
            403,
            "forbidden_host",
        ),
        (
            "Host が無い",
            Req {
                host: None,
                ..ok.clone()
            },
            403,
            "forbidden_host",
        ),
        (
            "Claude の待ち受けの Host",
            Req {
                host: Some("127.0.0.1:7338".into()),
                ..ok.clone()
            },
            403,
            "forbidden_host",
        ),
        (
            "別の Origin",
            Req {
                origin: Some("https://evil.example".into()),
                ..ok.clone()
            },
            403,
            "forbidden_origin",
        ),
        (
            "http の Origin",
            Req {
                origin: Some(format!("http://{}:8443", super::HOST)),
                ..ok.clone()
            },
            403,
            "forbidden_origin",
        ),
        (
            "null の Origin",
            Req {
                origin: Some("null".into()),
                ..ok.clone()
            },
            403,
            "forbidden_origin",
        ),
        (
            "ダッシュボードの Origin（同じ名前の 443）",
            Req {
                origin: Some(format!("https://{}", super::HOST)),
                ..ok.clone()
            },
            403,
            "forbidden_origin",
        ),
        (
            "Tailscale のヘッダーが無い",
            Req {
                login: None,
                ..ok.clone()
            },
            401,
            "no_tailnet_login",
        ),
        (
            "資格が無い",
            Req {
                cookie: None,
                ..ok.clone()
            },
            401,
            "no_device",
        ),
        (
            "登録に無い資格",
            Req {
                cookie: Some("mm_device=00ff".into()),
                ..ok.clone()
            },
            401,
            "unknown_device",
        ),
        (
            "別の人",
            Req {
                login: Some("other@example.com".into()),
                ..ok.clone()
            },
            401,
            "login_mismatch",
        ),
        (
            "X-MM-Mobile が無い",
            Req {
                mobile: false,
                ..ok.clone()
            },
            403,
            "missing_mobile_header",
        ),
    ];
    for (label, req, status, expected) in cases {
        let res = post_marker(&r, &req);
        assert_eq!(res.status, status, "{label}: {}", res.error_message());
        assert_eq!(code(&res), expected, "{label}");
    }
    assert_eq!(r.version(), 0);
    // 同じサイトのページが足した同じ名前のクッキーが前にあっても、登録した資格で通る。
    let tossed = Req::device(&format!("mm_device=00ff; {cookie}"));
    assert_eq!(post_marker(&r, &tossed).status, 200);
}

#[test]
fn 読み出しは_x_mm_mobile_なしで通り_資格は要る() {
    let r = rig();
    let cookie = r.pair("phone");
    let read = Req {
        mobile: false,
        ..Req::device(&cookie)
    };
    assert_eq!(r.send(&read, "GET", "/m/api/proposals", None).status, 200);
    let project = r.send(&read, "GET", "/m/api/project", None).json().unwrap();
    assert_eq!(project["version"], 0);
    assert_eq!(project["open_composition"], "main");
    assert!(
        project.get("assets").is_none() && project.get("project").is_none(),
        "素材のパスを返さない"
    );
    let story = r.send(&read, "GET", "/m/api/story/overview", None);
    assert_eq!(story.status, 200, "{}", story.error_message());
    let anonymous = Req {
        cookie: None,
        ..read
    };
    for path in ["/m/api/proposals", "/m/api/story/overview", "/m/api/events"] {
        let res = r.send(&anonymous, "GET", path, None);
        assert_eq!(code(&res), "no_device", "{path}");
    }
}

#[test]
fn 人の待ち受けに_mcp_と_claude_のパスは無い() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
    for path in ["/mcp", "/m/mcp", "/m/api/mcp", "/ops", "/proposals"] {
        assert_eq!(
            r.send(&req, "POST", path, Some(&init)).status,
            404,
            "{path}"
        );
    }
}

#[test]
fn 登録を外した端末は断り_外していない端末は通す() {
    let r = rig();
    let phone = r.pair("phone");
    let tablet = r.pair("tablet");
    let devices = r.mobile.devices().list().unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].login, LOGIN);
    assert!(r.mobile.devices().revoke(&devices[0].id).unwrap());
    assert!(!r.mobile.devices().revoke(&devices[0].id).unwrap());

    let res = post_marker(&r, &Req::device(&phone));
    assert_eq!((res.status, code(&res)), (401, "unknown_device".into()));
    assert_eq!(post_marker(&r, &Req::device(&tablet)).status, 200);
}

#[test]
fn 登録の応答は資格を_httponly_のクッキーにだけ入れ_ファイルにはハッシュだけ() {
    let r = rig();
    let code = r.mobile.devices().issue_code().unwrap().code;
    let res = r.send(
        &Req::tailnet(),
        "POST",
        "/m/pair",
        Some(&json!({"code": code, "device_name": "phone"})),
    );
    assert_eq!(res.status, 200, "{}", res.error_message());
    let set = res.header("set-cookie").unwrap();
    for attr in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/m"] {
        assert!(set.contains(attr), "{attr} が無い: {set}");
    }
    let credential = set.split(';').next().unwrap().split_once('=').unwrap().1;
    assert_eq!(credential.len(), mm_serve::CREDENTIAL_BYTES * 2);
    assert!(!String::from_utf8_lossy(&res.body).contains(credential));
    let file = std::fs::read_to_string(r.tmp.path().join("devices.json")).unwrap();
    assert!(!file.contains(credential));
    assert_eq!(res.json().unwrap()["device_name"], "phone");
}

fn try_pair(r: &Rig, code: &str, name: &str) -> mm_serve::Response {
    r.send(
        &Req::tailnet(),
        "POST",
        "/m/pair",
        Some(&json!({"code": code, "device_name": name})),
    )
}

#[test]
fn 登録コードは_1_回で消え_決まった回数を間違えると無効になる() {
    let r = rig();
    let first = r.mobile.devices().issue_code().unwrap().code;
    assert_eq!(try_pair(&r, &first, "phone").status, 200);
    assert_eq!(code(&try_pair(&r, &first, "tablet")), "pairing_unavailable");

    let second = r.mobile.devices().issue_code().unwrap().code;
    for _ in 0..PAIRING_MAX_FAILURES {
        assert_eq!(
            code(&try_pair(&r, "WRONG000", "tablet")),
            "pairing_wrong_code"
        );
    }
    assert_eq!(
        code(&try_pair(&r, &second, "tablet")),
        "pairing_unavailable"
    );
    assert_eq!(r.mobile.devices().list().unwrap().len(), 1);
}

#[test]
fn 期限の切れた登録コードでは登録できない() {
    let r = expired_rig();
    let code_ = r.mobile.devices().issue_code().unwrap().code;
    std::thread::sleep(std::time::Duration::from_millis(5));
    let res = try_pair(&r, &code_, "phone");
    assert_eq!(
        (res.status, code(&res)),
        (401, "pairing_unavailable".into())
    );
}

#[test]
fn 登録にも_tailnet_のヘッダーと_x_mm_mobile_が要る() {
    let r = rig();
    let c = r.mobile.devices().issue_code().unwrap().code;
    let body = json!({"code": c, "device_name": "phone"});
    let no_login = Req {
        login: None,
        ..Req::tailnet()
    };
    assert_eq!(
        code(&r.send(&no_login, "POST", "/m/pair", Some(&body))),
        "no_tailnet_login"
    );
    let no_header = Req {
        mobile: false,
        ..Req::tailnet()
    };
    assert_eq!(
        code(&r.send(&no_header, "POST", "/m/pair", Some(&body))),
        "missing_mobile_header"
    );
    assert_eq!(
        try_pair(&r, &c, "phone").status,
        200,
        "断った要求は登録コードを使わない"
    );
}
