//! Review パネルの Tauri のコマンド。名前と引数の形は `ui/sdk/README.md` の「Review のコマンド」。
//! 決定は人の操作経路（WebView）として `Core` を通るので、承認したトランザクションは Ctrl+Z で戻る。

use std::sync::Arc;

use mm_serve::{Approval, Caller, Core, ProposalView};
use mm_store::{Comment, CommentFilter, CommentState};
use tauri::{Emitter, State};

use crate::win::review::{self, drawing, Preview};
use crate::win::state::Shared;

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        review_list,
        review_approve,
        review_comment_focus,
        review_comment_reply,
        review_comment_state,
        review_comments,
        review_reject,
        review_reopen,
        review_undo,
        review_preview,
    ];
    h(invoke)
}

/// Review パネルが使うもの。
pub struct ReviewState {
    shared: Arc<Shared>,
}

impl ReviewState {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }

    fn core(&self) -> Result<Arc<Core>, String> {
        self.shared.core.get().cloned().ok_or_else(review::no_store)
    }
}

type Review<'a> = State<'a, Arc<ReviewState>>;

async fn blocking<T: Send + 'static>(
    state: Review<'_>,
    f: impl FnOnce(&ReviewState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| e.to_string())?
}

/// 決めた提案を Program に適用していれば、元のフレームに戻す（承認した後はデータ自体が提案を含む）。
fn end_preview(state: &ReviewState, id: i64) -> Result<(), String> {
    match state.shared.review_preview.get().map(|p| p.id) {
        Some(current) if current == id => review::set_preview(&state.shared, None),
        _ => Ok(()),
    }
}

/// 全部の提案（新しい順）。
#[tauri::command]
pub async fn review_list(state: Review<'_>) -> Result<Vec<ProposalView>, String> {
    blocking(state, |s| s.core()?.proposals().map_err(|e| e.to_string())).await
}

/// 保留中の提案を承認して適用する。`ops` は承認する操作の番号（`None` は全部）。
#[tauri::command]
pub async fn review_approve(
    state: Review<'_>,
    id: i64,
    ops: Option<Vec<usize>>,
) -> Result<Approval, String> {
    blocking(state, move |s| {
        let approval = s
            .core()?
            .approve(id, ops.as_deref(), &Caller::webview())
            .map_err(|e| e.to_string())?;
        end_preview(s, id)?;
        Ok(approval)
    })
    .await
}

/// 保留中の提案を却下する。
#[tauri::command]
pub async fn review_reject(state: Review<'_>, id: i64) -> Result<ProposalView, String> {
    blocking(state, move |s| {
        let proposal = s
            .core()?
            .reject(id, &Caller::webview())
            .map_err(|e| e.to_string())?;
        end_preview(s, id)?;
        Ok(proposal)
    })
    .await
}

/// 却下した提案を保留に戻す。
#[tauri::command]
pub async fn review_reopen(state: Review<'_>, id: i64) -> Result<ProposalView, String> {
    blocking(state, move |s| {
        s.core()?
            .reopen(id, &Caller::webview())
            .map_err(|e| e.to_string())
    })
    .await
}

/// 承認で適用したトランザクションが取り消しのスタックの一番上なら取り消す（Ctrl+Z と同じ経路）。
#[tauri::command]
pub async fn review_undo(state: Review<'_>, id: i64) -> Result<ProposalView, String> {
    blocking(state, move |s| {
        let core = s.core()?;
        review::undo_approved(&s.shared, &core, id)
    })
    .await
}

/// Program に提案を適用したフレームを出す。`id` が `None` なら元のフレームに戻す。
#[tauri::command]
pub async fn review_preview(
    state: Review<'_>,
    id: Option<i64>,
    ops: Option<Vec<usize>>,
) -> Result<(), String> {
    blocking(state, move |s| {
        let preview = id.map(|id| Preview { id, ops });
        review::set_preview(&s.shared, preview)
    })
    .await
}

/// コメントの追加・返信・状態の変化を Review パネルへ知らせる Tauri のイベント。中身は `mm_serve::CommentEvent`。
const COMMENTS_CHANGED_EVENT: &str = "mm:comments-changed";

/// コメントの通知を WebView へ転送するスレッドを始める。モバイルや Claude が書いたものも同じ道で届く。
/// 通知のたびに、Program に重ねる描き込みも読み直す。
pub fn forward_comments(app: &tauri::AppHandle, shared: &Arc<Shared>) {
    let Some(core) = shared.core.get() else {
        return;
    };
    drawing::reload(shared, core);
    let (handle, s) = (app.clone(), shared.clone());
    let started = core.on_comment(move |event| {
        if let Some(core) = s.core.get() {
            drawing::reload(&s, core);
        }
        let _ = handle.emit(COMMENTS_CHANGED_EVENT, event);
    });
    if let Err(e) = started {
        tracing::warn!("コメントの通知を受けるスレッドを始められない: {e}");
    }
}

/// Review で選んだコメント。その描き込みを、再生位置がコメントの時刻か範囲にある間だけ Program に重ねる。`None` で外す。
#[tauri::command]
pub async fn review_comment_focus(state: Review<'_>, id: Option<String>) -> Result<(), String> {
    state.shared.review_drawing.focus(id);
    Ok(())
}

/// 全部のコメント（書いた順）。絞り込みはパネルが持つ。
#[tauri::command]
pub async fn review_comments(state: Review<'_>) -> Result<Vec<Comment>, String> {
    blocking(state, |s| {
        s.core()?
            .comments(&CommentFilter::default())
            .map_err(|e| e.to_string())
    })
    .await
}

/// コメントに返信する。人の操作経路（`gui`）で書く。
#[tauri::command]
pub async fn review_comment_reply(
    state: Review<'_>,
    id: String,
    text: String,
) -> Result<Comment, String> {
    blocking(state, move |s| {
        s.core()?
            .reply_comment(&id, &text, &Caller::webview())
            .map_err(|e| e.to_string())
    })
    .await
}

/// コメントの状態を `open`、`done`、`wontfix` に決める。`fresh` は Claude の対応でだけなるので、`Core` が拒否する。
#[tauri::command]
pub async fn review_comment_state(
    state: Review<'_>,
    id: String,
    value: CommentState,
) -> Result<Comment, String> {
    blocking(state, move |s| {
        s.core()?
            .set_comment_state(&id, value, &Caller::webview())
            .map_err(|e| e.to_string())
    })
    .await
}
