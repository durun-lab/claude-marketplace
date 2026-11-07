//! モバイルの操作の権限。即時に適用するのは Story の操作と目印だけで、ほかは提案に回す。
//! 提案の承認・却下は人の操作として受け、Claude の MCP からは今のまま 403。

use mm_serve::Response;
use serde_json::{json, Value};

use super::{code, rig, Req, Rig};

fn add_marker(id: &str, secs: i64, base: i64) -> Value {
    json!({
        "op": {"op": "addMarker", "composition": "main",
               "marker": {"id": id, "time": {"num": secs, "den": 1}, "name": "見る"}},
        "base_version": base,
    })
}

fn opacity(layer: &str, base: i64) -> Value {
    json!({
        "op": {"op": "setParam", "target": {"layer": layer, "path": "opacity"},
               "value": {"type": "number", "value": 0.5}},
        "base_version": base,
    })
}

fn post(r: &Rig, req: &Req, path: &str, body: &Value) -> Response {
    r.send(req, "POST", path, Some(body))
}

#[test]
fn 目印は即時に適用され_発行元はモバイルの端末() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let res = post(&r, &req, "/m/api/ops", &add_marker("m1", 1, 0));
    assert_eq!(res.status, 200, "{}", res.error_message());
    assert_eq!(res.json().unwrap()["actor"], "mobile");
    assert_eq!(r.version(), 1);
}

#[test]
fn 表の外の操作は_not_on_mobile_で断り_提案なら出せる() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let res = post(&r, &req, "/m/api/ops", &opacity("l-footage", 0));
    assert_eq!((res.status, code(&res)), (403, "not_on_mobile".into()));
    assert_eq!(res.json().unwrap()["error"]["ops"], json!(["setParam"]));

    let mixed = json!({"txn": {"name": "混ぜた", "base_version": 0, "ops": [
        add_marker("m1", 1, 0)["op"], opacity("l-footage", 0)["op"],
    ]}});
    assert_eq!(code(&post(&r, &req, "/m/api/ops", &mixed)), "not_on_mobile");
    assert_eq!(r.version(), 0, "1 つでも表の外なら何も適用しない");

    let p = post(&r, &req, "/m/api/proposals", &opacity("l-footage", 0));
    assert_eq!(p.status, 200, "{}", p.error_message());
    let p = p.json().unwrap();
    assert_eq!(
        (p["actor"].as_str(), p["actor_name"].as_str()),
        (Some("mobile"), Some("phone"))
    );
    assert_eq!(p["state"], "pending");
    // シーケンスに当たらない提案でも、コンポジションの時刻の欄は空の列で返す。
    assert_eq!(p["comp_spans"], serde_json::json!([]));
    assert_eq!(r.version(), 0);
}

#[test]
fn base_version_と操作経路の名乗りは本体の形で決まる() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let mut no_base = add_marker("m1", 1, 0);
    no_base.as_object_mut().unwrap().remove("base_version");
    assert_eq!(post(&r, &req, "/m/api/ops", &no_base).status, 400);

    let mut spoof = add_marker("m1", 1, 0);
    spoof["entry"] = json!("cli");
    assert_eq!(post(&r, &req, "/m/api/ops", &spoof).status, 400);
    let mut actor = add_marker("m1", 1, 0);
    actor["actor"] = json!({"name": "claude"});
    assert_eq!(post(&r, &req, "/m/api/ops", &actor).status, 400);
    assert_eq!(r.version(), 0);
}

#[test]
fn ロックの付いたレイヤーはモバイルの提案でも触れない() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    // ロックを付けるのは PC の側（ロックを付ける操作そのものはロックに掛からない）。
    let lock = json!({"op": {"op": "lockLayer", "layer": "l-text", "lock": true}});
    assert_eq!(r.claude.post("/ops", &lock).unwrap().status, 200);
    let res = post(&r, &req, "/m/api/proposals", &opacity("l-text", 1));
    assert_eq!(
        (res.status, code(&res)),
        (403, "locked".into()),
        "{}",
        res.error_message()
    );
    let ok = post(&r, &req, "/m/api/proposals", &opacity("l-footage", 1));
    assert_eq!(ok.status, 200, "{}", ok.error_message());
}

fn claude_proposal(r: &Rig) -> i64 {
    let body = json!({"op": opacity("l-footage", 0)["op"], "base_version": 0, "entry": "mcp"});
    let res = r.claude.post("/proposals", &body).unwrap();
    assert_eq!(res.status, 200, "{}", res.error_message());
    res.json().unwrap()["id"].as_i64().unwrap()
}

#[test]
fn 提案の承認は人の待ち受けで受け_mcp_からは今のまま_403() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let id = claude_proposal(&r);

    let mcp = r
        .claude
        .post(
            &format!("/proposals/{id}/approve"),
            &json!({"entry": "mcp"}),
        )
        .unwrap();
    assert_eq!(mcp.status, 403, "{}", mcp.error_message());
    assert_eq!(r.version(), 0);

    let res = r.send(
        &req,
        "POST",
        &format!("/m/api/proposals/{id}/approve"),
        None,
    );
    assert_eq!(res.status, 200, "{}", res.error_message());
    let body = res.json().unwrap();
    assert_eq!(body["proposal"]["state"], "approved");
    assert_eq!(body["proposal"]["decided_by"], "mobile");
    assert_eq!(r.version(), 1);

    let undo = r.send(&req, "POST", "/m/api/undo", None);
    assert_eq!(undo.status, 200, "{}", undo.error_message());
    assert_eq!(undo.json().unwrap()["change"]["kind"], "undo");
}

#[test]
fn 却下と保留に戻すもモバイルから決められる() {
    let r = rig();
    let req = Req::device(&r.pair("phone"));
    let id = claude_proposal(&r);
    let rejected = r.send(&req, "POST", &format!("/m/api/proposals/{id}/reject"), None);
    assert_eq!(rejected.json().unwrap()["state"], "rejected");
    let reopened = r.send(&req, "POST", &format!("/m/api/proposals/{id}/reopen"), None);
    assert_eq!(reopened.json().unwrap()["state"], "pending");
    let with_ops = post(
        &r,
        &req,
        &format!("/m/api/proposals/{id}/reject"),
        &json!({"ops": [0]}),
    );
    assert_eq!(with_ops.status, 400);
}

#[test]
fn 取り消しはこの端末のトランザクションが直前のときだけ() {
    let r = rig();
    let phone = Req::device(&r.pair("phone"));
    let tablet = Req::device(&r.pair("tablet"));
    assert_eq!(
        post(&r, &phone, "/m/api/ops", &add_marker("m1", 1, 0)).status,
        200
    );

    let other = r.send(&tablet, "POST", "/m/api/undo", None);
    assert_eq!((other.status, code(&other)), (409, "not_latest".into()));
    let claude = r.claude.post("/ops", &opacity("l-footage", 1)).unwrap();
    assert_eq!(claude.status, 200, "{}", claude.error_message());
    assert_eq!(
        code(&r.send(&phone, "POST", "/m/api/undo", None)),
        "not_latest"
    );
    assert_eq!(r.version(), 2);

    r.claude.post("/undo", &json!({})).unwrap();
    let own = r.send(&phone, "POST", "/m/api/undo", None);
    assert_eq!(own.status, 200, "{}", own.error_message());
    assert_eq!(own.json().unwrap()["change"]["of"]["actor_name"], "phone");
}
