//! ホストの実行ファイルの本体。`clack-host` でプラグインを同じプロセスに読み込む。
//! このプロセスはアプリと分かれているので、プラグインのクラッシュはアプリに `HostCrashed` で届くだけで済む。
//! サブコマンドの振り分けと要求の繰り返しは `mm_host_pipe::audio::serve` の共通実装。

mod audio;
mod plugin;
mod serve;

use std::process::ExitCode;

use mm_host_pipe::audio::serve::HostMain;

/// サブコマンドを実行する。
pub fn run() -> ExitCode {
    HostMain {
        name: "mm-host-clap",
        usage: "使い方: mm-host-clap serve | mm-host-clap describe <.clap のファイル>",
        serve: serve::run,
        describe: plugin::describe_bundle,
    }
    .run()
}
