//! 端末の登録コードと端末の名前。登録コードを知らない者に名前を探らせず、並行の要求でも 1 台だけ登録する。

use std::sync::Arc;
use std::thread;

use serde_json::json;

use super::{code, rig, Req, Rig};

fn try_pair(r: &Rig, code: &str, name: &str) -> mm_serve::Response {
    r.send(
        &Req::tailnet(),
        "POST",
        "/m/pair",
        Some(&json!({"code": code, "device_name": name})),
    )
}

#[test]
fn 登録コードが違えば名前の重なりを明かさない() {
    let r = rig();
    r.pair("phone");
    r.mobile.devices().issue_code().unwrap();
    let res = try_pair(&r, "WRONG000", "phone");
    assert_eq!((res.status, code(&res)), (401, "pairing_wrong_code".into()));
}

#[test]
fn 名前の誤りでは登録コードを消さない() {
    let r = rig();
    r.pair("phone");
    let c = r.mobile.devices().issue_code().unwrap().code;
    for (name, expected) in [
        ("PHONE", "name_taken"),
        (" phone ", "name_taken"),
        ("", "bad_request"),
        ("tab\nlet", "bad_request"),
    ] {
        assert_eq!(code(&try_pair(&r, &c, name)), expected, "{name:?}");
    }
    assert_eq!(try_pair(&r, &c, "tablet").status, 200);
}

#[test]
fn クッキーを保存しなかった登録は同じ名前の登録し直しを塞がない() {
    let r = rig();
    let lost = r.pair_unused("iPhone");
    let kept = r.pair("iPhone");
    let devices = r.mobile.devices().list().unwrap();
    assert_eq!(devices.len(), 1, "{devices:?}");
    let res = r.send(&Req::device(&lost), "GET", "/m/api/project", None);
    assert_eq!((res.status, code(&res)), (401, "unknown_device".into()));
    let res = r.send(&Req::device(&kept), "GET", "/m/api/project", None);
    assert_eq!(res.status, 200);
}

#[test]
fn 手で入れた登録コードは_crockford_の読み替えをする() {
    let r = rig();
    let c = r.mobile.devices().issue_code().unwrap().code;
    // 0 と 1 を読み違えやすい文字に替えても通る。
    let typed: String = c
        .chars()
        .map(|ch| match ch {
            '0' => 'o',
            '1' => 'l',
            other => other,
        })
        .collect();
    assert_eq!(try_pair(&r, &typed, "phone").status, 200);
}

#[test]
fn 同じ登録コードの並行の登録は_1_台だけ通る() {
    const RACERS: usize = 8;
    let r = Arc::new(rig());
    let c = r.mobile.devices().issue_code().unwrap().code;
    let handles: Vec<_> = (0..RACERS)
        .map(|i| {
            let r = r.clone();
            let c = c.clone();
            thread::spawn(move || try_pair(&r, &c, &format!("phone{i}")).status)
        })
        .collect();
    let statuses: Vec<u16> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        statuses.iter().filter(|s| **s == 200).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|s| *s == 200 || *s == 401),
        "{statuses:?}"
    );
    assert_eq!(r.mobile.devices().list().unwrap().len(), 1);
}

#[test]
fn 資格の要らない読み出しも_host_と本人のヘッダーは確かめる() {
    let r = rig();
    let bad_host = Req {
        host: Some("evil.example".into()),
        ..Req::tailnet()
    };
    let no_login = Req {
        login: None,
        ..Req::tailnet()
    };
    // 資格は問わない（置き場の無い待ち受けでは PWA の画面が 404）。
    for (path, status) in [("/m/", 404), ("/m/pair", 404)] {
        assert_eq!(
            code(&r.send(&bad_host, "GET", path, None)),
            "forbidden_host"
        );
        assert_eq!(
            code(&r.send(&no_login, "GET", path, None)),
            "no_tailnet_login"
        );
        let res = r.send(&Req::tailnet(), "GET", path, None);
        assert_eq!(res.status, status, "{path}");
    }
}
