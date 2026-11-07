//! 自動の改善（docs/design/10-review-improve.md の「自動の改善」）。durun-dashboard の `server/improve.ts` の型を移す。
//!
//! - 受け付け: 人の要望（[`Improver::submit`]）と、エラーと遅い処理の自動の起票（[`Improver::record`]、[`ErrorBook`]）。
//!   同じものは 1 件にまとめ、未対応の上限と 1 時間あたりの上限を持つ。
//! - 起動: 動いている回がなく、ON で、未対応の要望があれば、別の worktree を作って [`Agent`]（本物は `claude -p`）に
//!   たまった要望をまとめて実装させる。
//! - 区切り: Stop フックで回を終え、最後の発言の「対応した要望:」から済んだ要望と返事を決める。使用制限で止まったら、
//!   明けたときに同じセッションで続ける。回の予算（ドルと時間）と、すぐ止める手段（[`Improver::stop`]）を持つ。
//! - 囲い: 回の変更を 1 コミットにまとめ、areas.toml の `[improve]` に収まるか（外なら PR までで止める）、
//!   [`Checks`]（cargo check、依存の逆引きの nextest、WebView の型、`mm perf compare`、`mm export` のハッシュ）を確かめて、
//!   通れば本線へ取り込む。
//!
//! 置き場所は `data_dir`（既定は `%LOCALAPPDATA%/movie-maker/improve`）。アプリと `mm improve` が同じものを共有する。

mod agent;
mod areas;
mod checks;
mod config;
mod errors;
mod files;
mod forge;
mod gate;
mod git;
mod improver;
mod proc;
mod prompt;
mod round;
mod state;
mod store;
mod time;

pub use agent::{
    find_transcript, stop_hook_settings, usage_limit, Agent, AgentEnd, AgentRequest, ClaudeCli,
    STOP_FILE,
};
pub use areas::{matches, Area, AreaReport, Areas, ImproveAreas, ImproveVerdict, Shared};
pub use checks::{
    affected_packages, parse_export_hash, Baseline, CargoChecks, CheckCx, CheckOutcome, Checks,
    Verdict,
};
pub use config::{
    ImproveConfig, Settings, SettingsPatch, DEFAULT_BUDGET_USD, DEFAULT_PERF_THRESHOLD,
    DEFAULT_WALL_MINUTES,
};
pub use errors::{
    key_of, ErrorBook, ErrorInput, ErrorRecord, ErrorSource, Load, RecentLanding, MAX_FILINGS,
    MAX_FILINGS_PER_HOUR, MAX_PENDING,
};
pub use forge::{Forge, GhForge, PullRequest};
pub use git::{changed_files, git};
pub use improver::{ImproveStatus, Improver, ListFilter, RunView};
pub use prompt::{parse_done, prompt_for, PromptContext, DONE_MARK};
pub use round::Step;
pub use state::{ImproveState, Phase, RunOutcome, RunReport, RunState};
pub use store::{
    FeedbackItem, FeedbackKind, FeedbackStatus, FeedbackStore, NewFeedback, Outcome, Reply,
    MAX_TEXT_CHARS,
};
