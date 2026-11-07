//! モバイルからの操作の即時の適用（`POST /m/api/ops`）と取り消し（`POST /m/api/undo`）。
//!
//! 即時に適用するのは Story で切る・並べる操作と目印（[`MOBILE_OPS`]）だけで、ほかは 403 `not_on_mobile`。
//! ほかの操作は提案（`POST /m/api/proposals`）で出し、PC かモバイルで承認する。

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use mm_ops::{Op, Transaction};
use mm_store::{Actor, Store};
use serde::Deserialize;
use serde_json::json;

use super::devices::DeviceInfo;
use super::guard::refuse;
use super::{MobileError, MobileState};
use crate::core::{Caller, Change, ChangeOf, Core, ServeError};
use crate::http::blocking;

/// モバイルから即時に適用できる操作（名前の順）。足すときは `docs/design/12-mobile.md` の「権限」の表と同じ PR で変える。
pub const MOBILE_OPS: &[&str] = &[
    "addMarker",
    "addSelection",
    "addTopic",
    "cutWords",
    "deleteWords",
    "fixTranscript",
    "insertTopics",
    "rate",
    "removeMarker",
    "removeSelection",
    "removeTopic",
    "tag",
];

pub(super) fn routes() -> Router<Arc<MobileState>> {
    Router::new()
        .route("/m/api/ops", post(post_ops))
        .route("/m/api/undo", post(post_undo))
}

/// 操作の本体。`txn` か `op` のどちらか 1 つで、`base_version` は必須。`entry` と `actor` は受けない。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MobileOpsBody {
    txn: Option<Transaction>,
    op: Option<Op>,
    name: Option<String>,
    base_version: Option<u64>,
}

impl MobileOpsBody {
    /// 本体を読み、トランザクションにする。どちらの形でも `base_version` が無ければ 400。
    pub(super) fn parse(body: &Bytes) -> Result<Transaction, ServeError> {
        let parsed: Self = serde_json::from_slice(body)
            .map_err(|e| ServeError::BadRequest(format!("本体の形が違う: {e}")))?;
        let txn = match (parsed.txn, parsed.op) {
            (Some(txn), None) if parsed.name.is_none() && parsed.base_version.is_none() => txn,
            (None, Some(op)) => {
                let mut txn = Transaction::new(parsed.name.unwrap_or_else(|| "op".into()), vec![op]);
                txn.base_version = parsed.base_version;
                txn
            }
            _ => {
                return Err(ServeError::BadRequest(
                    "txn か op のどちらか 1 つを渡す（txn のときは name と base_version を txn の中に書く）"
                        .into(),
                ))
            }
        };
        if txn.base_version.is_none() {
            return Err(ServeError::BadRequest(
                "モバイルからの操作は base_version が要る".into(),
            ));
        }
        Ok(txn)
    }
}

/// 即時に適用できない操作の名前（重ねずに）。空ならトランザクションを全部適用してよい。
pub(super) fn not_on_mobile(txn: &Transaction) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = txn
        .ops
        .iter()
        .map(Op::name)
        .filter(|n| !MOBILE_OPS.contains(n))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

async fn post_ops(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
    body: Bytes,
) -> Result<Response, MobileError> {
    let txn = MobileOpsBody::parse(&body)?;
    let refused = not_on_mobile(&txn);
    if !refused.is_empty() {
        return Ok(not_on_mobile_response(&refused));
    }
    let caller = device.caller();
    let core = st.core.clone();
    let change = blocking(move || core.apply(txn, &caller, None)).await?;
    Ok(Json(change).into_response())
}

fn not_on_mobile_response(ops: &[&str]) -> Response {
    tracing::warn!(?ops, "モバイルから即時に適用できない操作を断った");
    let message = format!(
        "{} はモバイルから即時に適用できない（提案で出す）",
        ops.join(", ")
    );
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": { "code": "not_on_mobile", "message": message, "ops": ops } })),
    )
        .into_response()
}

/// 取り消しは、この端末が出したトランザクションが全体の直前のときだけ。違えば 409 `not_latest`。
async fn post_undo(
    State(st): State<Arc<MobileState>>,
    Extension(device): Extension<DeviceInfo>,
) -> Result<Response, MobileError> {
    let caller = device.caller();
    let core = st.core.clone();
    match blocking(move || core.undo_own(&caller)).await? {
        UndoOwn::Done(change) => Ok(Json(json!({ "change": change })).into_response()),
        UndoOwn::NotLatest(of) => Err(refuse(
            StatusCode::CONFLICT,
            "not_latest",
            &format!(
                "直前のトランザクション「{}」は {}（{}）のもので、この端末のものでない",
                of.name, of.actor_name, of.actor
            ),
        )),
    }
}

enum UndoOwn {
    /// 取り消した（取り消すものが無ければ `None`）。
    Done(Option<Change>),
    /// 直前のトランザクションが別の人のもの。
    NotLatest(ChangeOf),
}

/// `actor` が出したか、`actor` が承認した提案のトランザクションか。承認の記録の発行元は提案した者なので、決めた者は提案の側で引く。
fn owned_by(s: &Store, target: &ChangeOf, actor: &Actor) -> Result<bool, ServeError> {
    if target.actor == actor.kind && target.actor_name == actor.name {
        return Ok(true);
    }
    Ok(s.proposals()?
        .iter()
        .any(|p| p.txn_id == Some(target.txn_id) && p.decided_by.as_ref() == Some(actor)))
}

impl Core {
    /// 直前のトランザクションが `caller` のものなら取り消す。確かめてから取り消すまで `Store` の列を離さない。
    fn undo_own(&self, caller: &Caller) -> Result<UndoOwn, ServeError> {
        self.with_store(|s| {
            let Some(target) = s.undo_plan()?.map(ChangeOf::from) else {
                return Ok(UndoOwn::Done(None));
            };
            if !owned_by(s, &target, &caller.actor)? {
                return Ok(UndoOwn::NotLatest(target));
            }
            let ctx = mm_ops::OpContext::new(caller.entry);
            let stored = s.undo_last(&ctx, caller.actor.clone(), crate::core::now_ms())?;
            Ok(UndoOwn::Done(
                stored.map(|t| self.finish(s, &t, Some(target))),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 即時の操作の表は名前の順で全部が操作の名前() {
        let mut sorted = MOBILE_OPS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, MOBILE_OPS);
        for name in MOBILE_OPS {
            assert!(Op::NAMES.contains(name), "{name} は操作に無い");
        }
    }
}
