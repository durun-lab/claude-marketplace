//! 人の待ち受けに載せたコメントの API。書くのは登録した端末の名前の人で、本文の発行元の指定は受けない。

use serde_json::json;

use super::{code, rig, Req};

#[test]
fn コメントは端末の名前の人として書け_発行元の指定は受けない() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let body = json!({"comp": "main", "at": {"num": 1, "den": 1}, "text": "ここを短く"});
    let res = r.send(&req, "POST", "/m/api/comments", Some(&body));
    assert_eq!(res.status, 200, "{}", res.error_message());
    let c = res.json().unwrap();
    assert_eq!(c["author"], json!({"kind": "mobile", "name": "phone"}));

    let mut spoof = body.clone();
    spoof["entry"] = json!("cli");
    let res = r.send(&req, "POST", "/m/api/comments", Some(&spoof));
    assert_eq!(res.status, 400);
    let anonymous = Req {
        cookie: None,
        ..req
    };
    let res = r.send(&anonymous, "POST", "/m/api/comments", Some(&body));
    assert_eq!(code(&res), "no_device");
    let list = r.claude.get("/comments").unwrap().json().unwrap();
    assert_eq!(list["comments"].as_array().unwrap().len(), 1);
}

#[test]
fn コメント_1_件を読み直すと返信が載り_未登録の端末は_401() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let body = json!({"comp": "main", "at": {"num": 1, "den": 1}, "text": "ここを短く"});
    let made = r.send(&req, "POST", "/m/api/comments", Some(&body));
    let id = made.json().unwrap()["id"].as_str().unwrap().to_owned();
    let reply = json!({"text": "どのくらい？"});
    let path = format!("/m/api/comments/{id}");
    let res = r.send(&req, "POST", &format!("{path}/replies"), Some(&reply));
    assert_eq!(res.status, 200, "{}", res.error_message());

    // 返信では状態が変わらないので、端末は SSE の `comment` を受けたら 1 件を読み直す。
    let one = r.send(&req, "GET", &path, None);
    assert_eq!(one.status, 200, "{}", one.error_message());
    let c = one.json().unwrap();
    assert_eq!(c["id"], json!(id));
    assert_eq!(c["replies"][0]["text"], json!("どのくらい？"));
    // 無い ID は Claude の待ち受けの `GET /comments/{id}` と同じ 400（ハンドラーを共有する）。
    let missing = r.send(&req, "GET", "/m/api/comments/nothing", None);
    assert_eq!(missing.status, 400);
    let anonymous = Req {
        cookie: None,
        ..req
    };
    assert_eq!(code(&r.send(&anonymous, "GET", &path, None)), "no_device");
}
