//! プロセスの木を作る偽の `claude`。tests/it/tree.rs が、アプリが落ちたときと回が終わったときに木が残らないことを確かめるのに使う。
//!
//! - `claude` として呼ばれたとき（引数に `-p`）: 孫のプロセス（`sleeper`）を起こし、自分と孫の pid を
//!   `MM_FAKE_CLAUDE_PIDS` に書く。`MM_FAKE_CLAUDE_EXIT` があればすぐ返事を出して終わり、無ければ眠り続ける。
//! - `host <data_dir> <repo>`: アプリに見立てる。自分を `claude` にした `Improver` で回を 1 つ回す。
//! - `sleeper`: 眠り続ける。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use mm_improve::{
    Baseline, CheckCx, CheckOutcome, Checks, ClaudeCli, GhForge, ImproveConfig, Improver,
};

/// 子と孫が眠る長さ。試験より十分長くし、止めなければ残ることを確かめられるようにする。
const SLEEP: Duration = Duration::from_secs(600);
const PIDS_ENV: &str = "MM_FAKE_CLAUDE_PIDS";
const EXIT_ENV: &str = "MM_FAKE_CLAUDE_EXIT";

/// 検査を全部通す。host は木を作るところまでしか進まない。
struct NoChecks;

impl Checks for NoChecks {
    fn baseline(&self, _cx: &CheckCx<'_>) -> mm_types::Result<Baseline> {
        Ok(Baseline::default())
    }

    fn verify(&self, _cx: &CheckCx<'_>, _baseline: &Baseline) -> Vec<CheckOutcome> {
        Vec::new()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sleeper") => std::thread::sleep(SLEEP),
        Some("host") => host(Path::new(&args[1]), Path::new(&args[2])),
        _ => claude(),
    }
}

fn host(data: &Path, repo: &Path) {
    let me = std::env::current_exe().expect("自分の実行ファイル");
    let agent = ClaudeCli {
        program: me,
        ..ClaudeCli::default()
    };
    let cfg = ImproveConfig::new(data, repo).with_subdir("mm");
    let imp = Improver::new(cfg, Arc::new(agent), Arc::new(NoChecks), Arc::new(GhForge));
    // 試験が木の pid を読んだらこのプロセスを止めるので、ここからは戻らない。
    drop(imp.run_once());
}

fn claude() {
    // 頼む文を読み切る。書き手が詰まらないようにする。
    drop(std::io::read_to_string(std::io::stdin()));
    let me = std::env::current_exe().expect("自分の実行ファイル");
    let child = Command::new(me)
        .arg("sleeper")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .expect("孫を起こす");
    if let Some(path) = std::env::var_os(PIDS_ENV).map(PathBuf::from) {
        // 試験が書きかけを読まないよう、別名に書いてから置き換える。
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, format!("{} {}", std::process::id(), child.id())).expect("pid");
        std::fs::rename(&tmp, &path).expect("pid");
    }
    finish(child);
}

/// `MM_FAKE_CLAUDE_EXIT` があれば孫を残したまま返事を出して終わる（claude が後ろに回したコマンドに見立てる）。
/// 無ければ孫を待つ（孫は眠り続けるので、止められるまで終わらない）。
fn finish(mut child: Child) {
    if std::env::var_os(EXIT_ENV).is_some() {
        println!(r#"{{"result":"対応した要望:","total_cost_usd":0}}"#);
        return;
    }
    drop(child.wait());
}
