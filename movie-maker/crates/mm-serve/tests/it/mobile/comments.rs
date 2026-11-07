//! 人の待ち受けに載せたコメントの API。書くのは登録した端末の名前の人で、本文の名乗りは受けない。

use serde_json::json;

use super::{code, rig, Req};

#[test]
fn コメントは端末の名前の人として書け_名乗りは受けない() {
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
