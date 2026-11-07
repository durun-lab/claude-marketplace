//! 別プロセスのホスト（`mm-host-cef`、`mm-host-ofx`、`mm-host-clap`）のアプリ側の接続と、大きなデータを渡す共有メモリ（[`shm`]）。
//!
//! ホストの実行ファイルを `serve` のサブコマンドで子プロセスとして起こし、標準入出力で
//! `HostRequest`/`HostResponse` の JSON を 1 行ずつやり取りする。ホストの実装ごとの違い
//! （一時フォルダー、子の環境変数、読み捨てる応答の共有ハンドルの扱い）は [`PipeKind`] で渡す。
//!
//! 依存の規則 3 の共通実装（RFC 0120）。インターフェースと基礎にだけ依存する。

mod job;
mod pipe;
pub mod shm;

pub use job::{kill_tree, Job};
pub use pipe::{remove_work_dir, PipeHost, PipeKind, StaleHandles, SERVE_COMMAND, SPEC_ENV};
