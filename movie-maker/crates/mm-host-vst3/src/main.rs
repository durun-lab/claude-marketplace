//! `mm-host-vst3` の実行ファイル。`serve` はアプリ（`Vst3Effect`）からの要求に答え、
//! `describe <バンドル>` は `.vst3` の Audio Module Class とパラメータの一覧を JSON の 1 行で出す。

mod host;

fn main() -> std::process::ExitCode {
    host::run()
}
