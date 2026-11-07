//! ホストの実行ファイルの本体。`vst3` クレートのバインディングで VST3 のモジュールを同じプロセスに読み込む。
//! このプロセスはアプリと分かれているので、プラグインのクラッシュはアプリに `HostCrashed` で届くだけで済む。
//! サブコマンドの振り分けと要求の繰り返しは `mm_host_pipe::audio::serve` の共通実装。

mod context;
mod module;
mod plugin;
mod process;
mod serve;

use std::process::ExitCode;

use mm_host_pipe::audio::serve::HostMain;

/// サブコマンドを実行する。
pub fn run() -> ExitCode {
    HostMain {
        name: "mm-host-vst3",
        usage: "使い方: mm-host-vst3 serve | mm-host-vst3 describe <.vst3 のバンドル>",
        serve: serve::run,
        describe: plugin::describe_bundle,
    }
    .run()
}
