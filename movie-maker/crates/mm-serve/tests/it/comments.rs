//! コメントの結合テスト。人が書き（`Core`。アプリと人の待ち受けの経路）、Claude が HTTP と MCP で読んで返信し、
//! 対応のトランザクションでコメントが `fresh` になる。

use std::path::Path;
use std::sync::Arc;

use mm_doc::Project;
use mm_serve::{Caller, Client, Core, Server};
use mm_store::{CommentFilter, CommentState, NewComment, Store};
use mm_types::RationalTime;
use serde_json::{json, Value};

const NOW: i64 = 1_700_000_000_000;

struct Rig {
    _tmp: tempfile::TempDir,
    core: Arc<Core>,
    _server: Server,
    client: Client,
}

fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/stage0/project.json");
    let project: Project = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let store = Store::create(&tmp.path().join("p"), &project, NOW).unwrap();
    let core = Core::new(store, None);
    let server = Server::start(core.clone()).unwrap();
    let client = Client::new(server.port(), server.token());
    Rig {
        _tmp: tmp,
        core,
        _server: server,
        client,
    }
}

fn at(secs: i64, text: &str) -> NewComment {
    NewComment {
        comp: "main".into(),
        client_id: None,
        at: Some(RationalTime::seconds(secs)),
        range: None,
        text: text.into(),
        drawing: vec![],
    }
}

fn mcp(r: &Rig, name: &str, arguments: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}});
    let res = r.client.post("/mcp", &body).unwrap();
    assert_eq!(res.status, 200);
    res.json().unwrap()["result"].clone()
}

fn mcp_json(result: &Value) -> Value {
    assert!(result.get("isError").is_none(), "{result}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn add_marker(id: &str) -> Value {
    json!({"op": "addMarker", "composition": "main",
        "marker": {"id": id, "time": {"num": 1, "den": 1}, "name": "m"}})
}

#[test]
fn 人が書いたコメントを_claude_が一覧と_1_件で読み_返信できる() {
    let r = rig();
    let human = Caller::webview();
    r.core.add_comment(&at(2, "ここ暗い"), &human).unwrap();
    r.core.add_comment(&at(8, "音が大きい"), &human).unwrap();

    let list = r.client.get("/comments").unwrap().json().unwrap();
    assert_eq!(list["comments"].as_array().unwrap().len(), 2);
    assert_eq!(list["comments"][0]["text"], "ここ暗い");
    assert_eq!(list["comments"][0]["author"]["kind"], "gui");

    let near = r
        .client
        .get("/comments?from=1&to=3&state=open")
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(near["comments"].as_array().unwrap().len(), 1);
    assert_eq!(near["comments"][0]["id"], "c-1");
    assert_eq!(r.client.get("/comments?from=1").unwrap().status, 400);
    assert_eq!(r.client.get("/comments/c-9").unwrap().status, 400);

    let reply = r
        .client
        .post("/comments/c-1/replies", &json!({"text": "明るくします"}))
        .unwrap();
    assert_eq!(reply.status, 200, "{}", reply.error_message());
    let c = reply.json().unwrap();
    assert_eq!(c["replies"][0]["text"], "明るくします");
    assert_eq!(c["replies"][0]["author"]["kind"], "cli");
    assert_eq!(c["state"], "open", "返信は状態を変えない");
    let one = r.client.get("/comments/c-1").unwrap().json().unwrap();
    assert_eq!(one["replies"].as_array().unwrap().len(), 1);
}

#[test]
fn claude_の待ち受けは書くことと状態を決めることを断る() {
    let r = rig();
    r.core
        .add_comment(&at(2, "ここ暗い"), &Caller::webview())
        .unwrap();
    let write = r
        .client
        .post(
            "/comments",
            &json!({"comp": "main", "at": {"num": 1, "den": 1}, "text": "x"}),
        )
        .unwrap();
    assert_eq!(write.status, 403);
    assert!(
        write
            .error_message()
            .starts_with("コメントを書くのは人の操作経路だけ"),
        "{}",
        write.error_message()
    );
    for entry in ["cli", "mcp"] {
        let state = r
            .client
            .post(
                "/comments/c-1/state",
                &json!({"state": "done", "entry": entry}),
            )
            .unwrap();
        assert_eq!(state.status, 403, "{entry}");
    }
    let c = r.core.comment("c-1").unwrap();
    assert_eq!(c.state, CommentState::Open);
}

#[test]
fn mcp_の_comments_と_comment_reply() {
    let r = rig();
    let human = Caller::webview();
    r.core.add_comment(&at(2, "ここ暗い"), &human).unwrap();
    r.core.add_comment(&at(8, "音が大きい"), &human).unwrap();
    r.core
        .set_comment_state("c-2", CommentState::Done, &human)
        .unwrap();

    let open = mcp_json(&mcp(&r, "comments", json!({"state": "open"})));
    assert_eq!(open["comments"].as_array().unwrap().len(), 1);
    assert_eq!(open["comments"][0]["id"], "c-1");
    let one = mcp_json(&mcp(&r, "comments", json!({"id": "c-2"})));
    assert_eq!(one["state"], "done");
    let mixed = mcp(&r, "comments", json!({"id": "c-2", "state": "open"}));
    assert_eq!(mixed["isError"], true);
    let missing = mcp(&r, "comments", json!({"id": "c-9"}));
    assert_eq!(missing["isError"], true);

    let reply = mcp_json(&mcp(
        &r,
        "comment_reply",
        json!({"id": "c-1", "text": "直しました"}),
    ));
    assert_eq!(reply["replies"][0]["author"]["kind"], "mcp");
    assert_eq!(reply["replies"][0]["author"]["name"], "claude");
    let empty = mcp(&r, "comment_reply", json!({"id": "c-1", "text": " "}));
    assert_eq!(empty["isError"], true);
    let list = r
        .core
        .comments(&CommentFilter {
            state: Some(CommentState::Open),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(list[0].replies.len(), 1);
}

#[test]
fn 対応の_transaction_でコメントが_fresh_になり_comment_が_events_に流れる() {
    let r = rig();
    r.core
        .add_comment(&at(2, "ここ暗い"), &Caller::webview())
        .unwrap();
    let mut events = r.client.events().unwrap();
    assert_eq!(events.next_event().unwrap().unwrap().event, "hello");

    let result = mcp(
        &r,
        "transaction",
        json!({"name": "対応", "ops": [add_marker("a")], "responds_to": "c-1"}),
    );
    let change = mcp_json(&result);
    let txn_id = change["txn_id"].as_i64().unwrap();

    let mut seen = Vec::new();
    while seen.len() < 2 {
        seen.push(events.next_event().unwrap().unwrap());
    }
    let comment = seen.iter().find(|e| e.event == "comment").unwrap();
    assert_eq!(comment.data, json!({"id": "c-1", "state": "fresh"}));
    assert!(seen.iter().any(|e| e.event == "change"));

    let c = r.client.get("/comments/c-1").unwrap().json().unwrap();
    assert_eq!(c["state"], "fresh");
    assert_eq!(c["responded_txn"], txn_id);
}

#[test]
fn 提案の承認でコメントが_fresh_になる() {
    let r = rig();
    let human = Caller::webview();
    r.core.add_comment(&at(2, "ここ暗い"), &human).unwrap();
    let proposal = mcp_json(&mcp(
        &r,
        "propose",
        json!({"name": "提案", "ops": [add_marker("p")], "responds_to": "c-1"}),
    ));
    assert_eq!(r.core.comment("c-1").unwrap().state, CommentState::Open);

    let mut events = r.client.events().unwrap();
    events.next_event().unwrap();
    let id = proposal["id"].as_i64().unwrap();
    let approved = r
        .client
        .post(&format!("/proposals/{id}/approve"), &json!({}))
        .unwrap();
    assert_eq!(approved.status, 200, "{}", approved.error_message());
    let txn_id = approved.json().unwrap()["change"]["txn_id"]
        .as_i64()
        .unwrap();

    let mut names = Vec::new();
    while names.len() < 3 {
        let e = events.next_event().unwrap().unwrap();
        if e.event == "comment" {
            assert_eq!(e.data, json!({"id": "c-1", "state": "fresh"}));
        }
        names.push(e.event);
    }
    assert!(names.contains(&"comment".to_string()), "{names:?}");
    let c = r.core.comment("c-1").unwrap();
    assert_eq!(c.responded_txn, Some(txn_id));
}

#[test]
fn 人の操作の通知は_comment_として流れ_済みのコメントは対応で動かない() {
    let r = rig();
    let human = Caller::webview();
    let mut events = r.client.events().unwrap();
    events.next_event().unwrap();
    r.core.add_comment(&at(2, "ここ暗い"), &human).unwrap();
    let e = events.next_event().unwrap().unwrap();
    assert_eq!(
        (e.event.as_str(), e.data),
        ("comment", json!({"id": "c-1", "state": "open"}))
    );
    r.core
        .set_comment_state("c-1", CommentState::Wontfix, &human)
        .unwrap();
    let e = events.next_event().unwrap().unwrap();
    assert_eq!(e.data, json!({"id": "c-1", "state": "wontfix"}));

    mcp_json(&mcp(
        &r,
        "transaction",
        json!({"name": "対応", "ops": [add_marker("a")], "responds_to": "c-1"}),
    ));
    assert_eq!(r.core.comment("c-1").unwrap().state, CommentState::Wontfix);
}
