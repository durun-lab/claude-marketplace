//! コメントを `Core` の直列の列で扱い、HTTP（`/comments`）に出す。
//!
//! 書いて決めるのは人の操作経路だけで、Claude（`cli`、`mcp`）は読むのと返信だけ。変更は [`CommentEvent`] で
//! 購読者（`GET /events` の `comment`）に送る。Claude の対応（`responds_to` つきの適用と提案の承認）で
//! コメントが `fresh` になったときも同じ通知を送る。

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use mm_store::{Comment, CommentFilter, CommentState, NewComment, Shape, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;

use crate::core::{now_ms, Caller, Core, ServeError};
use crate::frames::parse_time;
use crate::http::{blocking, ActorBody, AppState, OpsBody};

/// コメントの追加と状態の変化の通知。`GET /events` の `comment` が送る。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommentEvent {
    pub id: String,
    pub state: CommentState,
}

/// 書く、決める、描き込むは人の操作経路だけ。Claude が自分で `done` にすると、人が確かめる段が無くなる。
fn require_human(caller: &Caller, what: &str) -> Result<(), ServeError> {
    if caller.entry.is_claude() {
        return Err(ServeError::Forbidden(format!(
            "{what}は人の操作経路だけ。Claude はコメントを読んで返信する"
        )));
    }
    Ok(())
}

impl Core {
    /// コメントの通知を受ける。
    pub fn subscribe_comments(&self) -> broadcast::Receiver<CommentEvent> {
        self.comments.subscribe()
    }

    /// コメントの通知のたびに `f` を呼ぶスレッドを始める。アプリが Review パネルへの通知に使う（`on_proposal` と同じ形）。
    pub fn on_comment(
        &self,
        f: impl FnMut(CommentEvent) + Send + 'static,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        crate::review::listen("serve-comments", "コメント", self.subscribe_comments(), f)
    }

    fn announce_comment(&self, c: &Comment) {
        // 購読者がいなければ送り先が無いだけなので、失敗は無視する。
        let _ = self.comments.send(CommentEvent {
            id: c.id.clone(),
            state: c.state,
        });
    }

    /// 対応で `fresh` になったコメントを通知する。`txn_id` を `responded_txn` に持つときだけ（済みのコメントは動かない）。
    pub(crate) fn announce_response(&self, store: &Store, responds_to: Option<&str>, txn_id: i64) {
        let Some(id) = responds_to else { return };
        if let Ok(c) = store.comment(id) {
            if c.responded_txn == Some(txn_id) {
                self.announce_comment(&c);
            }
        }
    }

    /// 条件に合うコメントを書いた順に返す。
    pub fn comments(&self, filter: &CommentFilter) -> Result<Vec<Comment>, ServeError> {
        self.with_store(|s| Ok(s.list_comments(filter)?))
    }

    /// コメント 1 件。
    pub fn comment(&self, id: &str) -> Result<Comment, ServeError> {
        self.with_store(|s| Ok(s.comment(id)?))
    }

    /// コメントを書く（人だけ）。
    pub fn add_comment(&self, new: &NewComment, caller: &Caller) -> Result<Comment, ServeError> {
        require_human(caller, "コメントを書くのは")?;
        let c = self.with_store(|s| Ok(s.add_comment(new, &caller.actor, now_ms())?))?;
        self.announce_comment(&c);
        Ok(c)
    }

    /// 返信する。人も Claude もできる。状態は変えない。
    pub fn reply_comment(
        &self,
        id: &str,
        text: &str,
        caller: &Caller,
    ) -> Result<Comment, ServeError> {
        let c = self.with_store(|s| Ok(s.reply_comment(id, &caller.actor, text, now_ms())?))?;
        self.announce_comment(&c);
        Ok(c)
    }

    /// 状態を `open`、`done`、`wontfix` に決める（人だけ）。
    pub fn set_comment_state(
        &self,
        id: &str,
        state: CommentState,
        caller: &Caller,
    ) -> Result<Comment, ServeError> {
        require_human(caller, "コメントの状態を決めるのは")?;
        let c = self.with_store(|s| Ok(s.set_comment_state(id, state)?))?;
        self.announce_comment(&c);
        Ok(c)
    }

    /// 描き込みを足す（人だけ）。
    pub fn attach_drawing(
        &self,
        id: &str,
        shapes: &[Shape],
        caller: &Caller,
    ) -> Result<Comment, ServeError> {
        require_human(caller, "描き込みを足すのは")?;
        let c = self.with_store(|s| Ok(s.attach_drawing(id, shapes)?))?;
        self.announce_comment(&c);
        Ok(c)
    }
}

/// コメントのルーター。デスクトップの待ち受け（`router` が `guard` の内側に足す）にも、人の待ち受けの `/m/api` にも
/// 同じパスで載せる。人の待ち受けは認証の層が `Extension<Caller>`（端末の名前の人）を入れ、無ければ本体の
/// `entry`（`cli` か `mcp`）で Claude の呼び出しとして扱う。
pub(crate) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/comments", get(list).post(create))
        .route("/comments/{id}", get(one))
        .route("/comments/{id}/replies", post(reply))
        .route("/comments/{id}/state", post(set_state))
}

/// `GET /comments` のクエリ。`from` と `to` は時刻（`1.5`、`3/2`）で、両方を渡すとその区間と重なるものだけ。
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListQuery {
    pub state: Option<CommentState>,
    pub comp: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl ListQuery {
    /// 絞り込みの条件を 1 つでも持つか。
    pub(crate) fn is_set(&self) -> bool {
        self.state.is_some() || self.comp.is_some() || self.from.is_some() || self.to.is_some()
    }

    /// クエリを絞り込みの条件にする。`from` か `to` の片方だけは拒否する。
    pub(crate) fn filter(&self) -> Result<CommentFilter, ServeError> {
        let overlaps = match (&self.from, &self.to) {
            (Some(a), Some(b)) => Some((parse_time(a)?, parse_time(b)?)),
            (None, None) => None,
            _ => return Err(ServeError::BadRequest("from と to は両方渡す".into())),
        };
        Ok(CommentFilter {
            state: self.state,
            comp: self.comp.clone(),
            overlaps,
        })
    }
}

async fn list(
    State(st): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Response, ServeError> {
    let filter = q.filter()?;
    let core = st.core.clone();
    let comments = blocking(move || core.comments(&filter)).await?;
    Ok(Json(json!({ "comments": comments })).into_response())
}

async fn one(
    State(st): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, ServeError> {
    let core = st.core.clone();
    let c = blocking(move || core.comment(&id)).await?;
    Ok(Json(c).into_response())
}

/// 呼び出し側。人の待ち受けの認証の層が入れた `Caller` を使い、無ければ本体の `entry` と `actor`（Claude 側）から決める。
fn caller_of(
    ext: Option<Extension<Caller>>,
    entry: Option<String>,
    actor: Option<ActorBody>,
) -> Result<Caller, ServeError> {
    match ext {
        Some(Extension(caller)) if entry.is_none() && actor.is_none() => Ok(caller),
        Some(_) => Err(ServeError::BadRequest(
            "人の待ち受けでは entry と actor は渡せない".into(),
        )),
        None => OpsBody {
            txn: None,
            op: None,
            name: None,
            base_version: None,
            entry,
            actor,
            responds_to: None,
        }
        .caller(),
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ServeError> {
    serde_json::from_slice(body).map_err(|e| ServeError::BadRequest(format!("本体の形が違う: {e}")))
}

/// `POST /comments` の本体。`NewComment` の項目に、デスクトップの待ち受けで操作経路を決める `entry` と `actor`。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBody {
    comp: String,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    at: Option<mm_types::RationalTime>,
    #[serde(default)]
    range: Option<mm_store::CommentRange>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    drawing: Vec<Shape>,
    entry: Option<String>,
    actor: Option<ActorBody>,
}

async fn create(
    State(st): State<Arc<AppState>>,
    ext: Option<Extension<Caller>>,
    body: Bytes,
) -> Result<Response, ServeError> {
    let b: CreateBody = parse(&body)?;
    let caller = caller_of(ext, b.entry, b.actor)?;
    let new = NewComment {
        client_id: b.client_id,
        comp: b.comp,
        at: b.at,
        range: b.range,
        text: b.text,
        drawing: b.drawing,
    };
    let core = st.core.clone();
    let c = blocking(move || core.add_comment(&new, &caller)).await?;
    Ok(Json(c).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyBody {
    text: String,
    entry: Option<String>,
    actor: Option<ActorBody>,
}

async fn reply(
    State(st): State<Arc<AppState>>,
    Path(id): Path<String>,
    ext: Option<Extension<Caller>>,
    body: Bytes,
) -> Result<Response, ServeError> {
    let b: ReplyBody = parse(&body)?;
    let caller = caller_of(ext, b.entry, b.actor)?;
    let core = st.core.clone();
    let c = blocking(move || core.reply_comment(&id, &b.text, &caller)).await?;
    Ok(Json(c).into_response())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateBody {
    state: CommentState,
    entry: Option<String>,
    actor: Option<ActorBody>,
}

async fn set_state(
    State(st): State<Arc<AppState>>,
    Path(id): Path<String>,
    ext: Option<Extension<Caller>>,
    body: Bytes,
) -> Result<Response, ServeError> {
    let b: StateBody = parse(&body)?;
    let caller = caller_of(ext, b.entry, b.actor)?;
    let core = st.core.clone();
    let c = blocking(move || core.set_comment_state(&id, b.state, &caller)).await?;
    Ok(Json(c).into_response())
}

#[cfg(test)]
mod tests {
    //! 人の待ち受け（`/m/api`）への載せ方の確かめ。認証の層は別の担当が作るので、ここでは層の代わりに
    //! `Extension<Caller>` を入れる関数を挟む。

    use std::net::{Ipv4Addr, SocketAddr};

    use axum::extract::Request;
    use axum::middleware::{self, Next};
    use mm_doc::{Composition, Project};
    use mm_store::Actor;
    use mm_types::{Id, Rational, RationalTime};
    use tokio::runtime::Runtime;

    use super::*;
    use crate::Client;

    async fn as_phone(mut req: Request, next: Next) -> Response {
        req.extensions_mut().insert(Caller {
            entry: mm_ops::Entry::WebView,
            actor: Actor {
                kind: "mobile".into(),
                name: "Pixel".into(),
            },
        });
        next.run(req).await
    }

    struct Phone {
        _rt: Runtime,
        _tmp: tempfile::TempDir,
        core: Arc<Core>,
        client: Client,
    }

    fn phone() -> Phone {
        let tmp = tempfile::tempdir().unwrap();
        let mut project = Project::new();
        project.compositions.push(Composition::new(
            Id::new("main"),
            320,
            180,
            Rational::integer(30),
            RationalTime::seconds(10),
        ));
        let store = Store::create(&tmp.path().join("p"), &project, 0).unwrap();
        let core = Core::new(store, None);
        let state = Arc::new(AppState {
            core: core.clone(),
            token: String::new(),
            hosts: vec![],
        });
        let app = Router::new()
            .nest("/m/api", routes())
            .layer(middleware::from_fn(as_phone))
            .with_state(state);
        let rt = Runtime::new().unwrap();
        let listener = rt
            .block_on(tokio::net::TcpListener::bind(SocketAddr::from((
                Ipv4Addr::LOCALHOST,
                0,
            ))))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        rt.spawn(async move { axum::serve(listener, app).await });
        Phone {
            _rt: rt,
            _tmp: tmp,
            core,
            client: Client::new(port, ""),
        }
    }

    #[test]
    fn 人の待ち受けは端末の名前で書き_返信し_状態を決められる() {
        let p = phone();
        let mut rx = p.core.subscribe_comments();
        let made = p
            .client
            .post(
                "/m/api/comments",
                &json!({
                    "comp": "main", "at": {"num": 3, "den": 2}, "text": "ここ",
                    "drawing": [{"kind": "rect", "points": [[0.1, 0.1], [0.5, 0.4]]}],
                }),
            )
            .unwrap();
        assert_eq!(made.status, 200, "{}", made.error_message());
        let c = made.json().unwrap();
        assert_eq!(c["author"], json!({"kind": "mobile", "name": "Pixel"}));
        assert_eq!(c["drawing"][0]["kind"], "rect");
        assert_eq!(rx.try_recv().unwrap().state, CommentState::Open);

        let r = p
            .client
            .post("/m/api/comments/c-1/replies", &json!({"text": "追記"}))
            .unwrap();
        assert_eq!(r.json().unwrap()["replies"][0]["author"]["name"], "Pixel");
        let s = p
            .client
            .post("/m/api/comments/c-1/state", &json!({"state": "done"}))
            .unwrap();
        assert_eq!(s.json().unwrap()["state"], "done");
        let fresh = p
            .client
            .post("/m/api/comments/c-1/state", &json!({"state": "fresh"}))
            .unwrap();
        assert_eq!(fresh.status, 400, "fresh は決められない");
        let list = p.client.get("/m/api/comments?state=done").unwrap();
        assert_eq!(list.json().unwrap()["comments"][0]["id"], "c-1");
        let open = p.client.get("/m/api/comments?state=open").unwrap();
        assert!(open.json().unwrap()["comments"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn 人の待ち受けは本体の_entry_を受けない() {
        let p = phone();
        let res = p
            .client
            .post(
                "/m/api/comments",
                &json!({"comp": "main", "at": {"num": 1, "den": 1}, "text": "x", "entry": "cli"}),
            )
            .unwrap();
        assert_eq!(res.status, 400);
        assert!(p
            .core
            .comments(&CommentFilter::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn コメントの通知のたびに_on_comment_の関数が呼ばれる() {
        const WAIT: std::time::Duration = std::time::Duration::from_secs(5);
        let p = phone();
        let (tx, rx) = std::sync::mpsc::channel();
        p.core
            .on_comment(move |e| {
                let _ = tx.send(e);
            })
            .unwrap();
        let made = p
            .client
            .post(
                "/m/api/comments",
                &json!({"comp": "main", "at": {"num": 1, "den": 1}, "text": "x"}),
            )
            .unwrap();
        assert_eq!(made.status, 200, "{}", made.error_message());
        let event = rx.recv_timeout(WAIT).unwrap();
        assert_eq!(event.id, "c-1");
        assert_eq!(event.state, CommentState::Open);
    }
}
