//! `mm-host-clap` の実行ファイル。`serve` はアプリ（`ClapEffect`）からの要求に答え、
//! `describe <ファイル>` は `.clap` のプラグインとパラメータの一覧を JSON の 1 行で出す。

mod host;

fn main() -> std::process::ExitCode {
    host::run()
}
