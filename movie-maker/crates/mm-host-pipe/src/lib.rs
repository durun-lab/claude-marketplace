//! 別プロセスのホスト（`mm-host-cef`、`mm-host-ofx`、`mm-host-clap`、`mm-host-vst3`）のアプリ側の接続と、
//! 大きなデータを渡す共有メモリ（[`shm`]）、音のプラグインのホストの共通部分（[`audio`]）。
//!
//! ホストの実行ファイルを `serve` のサブコマンドで子プロセスとして起こし、標準入出力で
//! `HostRequest`/`HostResponse` の JSON を 1 行ずつやり取りする。ホストの実装ごとの違い
//! （一時フォルダー、子の環境変数、読み捨てる応答の共有ハンドルの扱い）は [`PipeKind`] で渡す。
//! プラグインを読むホストに共通の部分（`describe` の実行、置き場所の決まり、共有ライブラリの読み込み）もここに置く。
//!
//! 依存の規則 3 の共通実装（RFC 0120）。インターフェースと基礎にだけ依存する。

pub mod audio;
mod describe;
mod dll;
mod job;
mod locate;
mod pipe;
pub mod shm;

pub use describe::{run_describe, DESCRIBE_COMMAND};
pub use dll::load_library;
pub use job::{kill_tree, wait_exit, Job};
pub use locate::{find_executable, resolve_in_roots};
pub use pipe::{remove_work_dir, PipeHost, PipeKind, StaleHandles, SERVE_COMMAND, SPEC_ENV};
