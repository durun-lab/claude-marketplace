//! モバイル版の通しの確認（`ui/mobile/e2e/`）が立てる PC の側。本物の `Core`、Claude の待ち受け、人の待ち受けを
//! フィクスチャのプロジェクトで始め、標準出力に 1 行の JSON で待ち受けを知らせる。
//!
//! 標準入力の 1 行ごとに応える。`code` は PC のアプリの Devices の「Add device」と同じ `Devices::issue_code`
//! で登録の符号を出し、`devices` は登録した端末の一覧を出す。標準入力が閉じたら止める。
//!
//! 使い方: `cargo run -p mm-testkit --example mobile_rig --features mm-runtime/ffmpeg -- \
//!   --project fixtures/projects/mobile/project.json --transcript fixtures/ingest/transcript-ja.json \
//!   --assets ui/mobile/dist --work <空のフォルダー>`
//!
//! `Server::start` はトークンを `%LOCALAPPDATA%\movie-maker\` に置くので、呼ぶ側は `LOCALAPPDATA` を差し替える。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_runtime::RuntimeConfig;
use mm_serve::{
    Core, DirAssets, EngineFrames, FrameSource, MobileConfig, MobileServer, Server, MOBILE_PORT,
};
use mm_store::Store;
use serde_json::{json, Value};

/// 試験の `Host` に許す tailnet での名前の既定。
const DEFAULT_HOST: &str = "desk.tail1234.ts.net";
/// 文字起こしの付属ファイルの置き場（プロジェクトのフォルダーからの相対）。`mm-story` がここを読む。
const TRANSCRIPT_DIR: &str = "cache/ingest";

struct Args {
    project: PathBuf,
    transcript: Option<PathBuf>,
    assets: Option<PathBuf>,
    work: PathBuf,
    host: String,
    port: u16,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        project: PathBuf::new(),
        transcript: None,
        assets: None,
        work: PathBuf::new(),
        host: DEFAULT_HOST.into(),
        port: MOBILE_PORT,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} の値が無い"))?;
        match flag.as_str() {
            "--project" => args.project = value.into(),
            "--transcript" => args.transcript = Some(value.into()),
            "--assets" => args.assets = Some(value.into()),
            "--work" => args.work = value.into(),
            "--host" => args.host = value,
            "--port" => args.port = value.parse().map_err(|e| format!("--port: {e}"))?,
            other => return Err(format!("知らない引数 {other}")),
        }
    }
    if args.project.as_os_str().is_empty() || args.work.as_os_str().is_empty() {
        return Err("--project と --work が要る".into());
    }
    Ok(args)
}

/// フィクスチャを今のバージョンへ上げて `project.mmdb` を作り、内容のハッシュが合う素材に文字起こしを置く。
fn create_store(args: &Args, dir: &Path) -> Result<Store, String> {
    let json = std::fs::read_to_string(&args.project).map_err(|e| e.to_string())?;
    let project = mm_doc::load(&json).map_err(|e| e.to_string())?;
    if let Some(transcript) = &args.transcript {
        let raw = std::fs::read_to_string(transcript).map_err(|e| e.to_string())?;
        let hash = serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string())?["content_hash"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let sidecar = dir
            .join(TRANSCRIPT_DIR)
            .join(format!("{hash}.transcript.json"));
        std::fs::create_dir_all(sidecar.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        std::fs::write(sidecar, raw).map_err(|e| e.to_string())?;
    }
    Store::create(dir, &project, mm_serve::now_ms()).map_err(|e| e.to_string())
}

/// アプリと同じ実際の実装でコマと HLS を描く。組めなければコマと映像の道だけ使えないまま続ける。
fn frames(dir: &Path) -> Option<Arc<dyn FrameSource>> {
    let config = RuntimeConfig {
        present: false,
        ..RuntimeConfig::real()
    };
    match EngineFrames::spawn(config, dir.join("cache/mobile")) {
        Ok(f) => Some(Arc::new(f)),
        Err(e) => {
            eprintln!("描画を始められない（コマと HLS は使えない）: {e}");
            None
        }
    }
}

fn mobile_config(args: &Args) -> MobileConfig {
    let mut config = MobileConfig::new(vec![args.host.clone()], args.work.join("devices.json"));
    config.port = args.port;
    config.assets = args
        .assets
        .as_ref()
        .map(|dir| Arc::new(DirAssets::new(dir)) as Arc<dyn mm_serve::MobileAssets>);
    config
}

fn say(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// 標準入力の 1 行の命令に応える。
fn answer(mobile: &MobileServer, line: &str) -> Value {
    let devices = mobile.devices();
    match line.trim() {
        "code" => match devices.issue_code() {
            Ok(code) => json!({ "code": code.code, "url": mobile.pairing_url(&code) }),
            Err(e) => json!({ "error": e.to_string() }),
        },
        "devices" => match devices.list() {
            Ok(list) => json!({ "devices": list }),
            Err(e) => json!({ "error": e.to_string() }),
        },
        other => json!({ "error": format!("知らない命令 {other}") }),
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let dir = args.work.join("p");
    let store = create_store(&args, &dir)?;
    let core = Core::new(store, frames(&dir));
    let server = Server::start(core.clone()).map_err(|e| e.to_string())?;
    let mobile = MobileServer::start(core, mobile_config(&args)).map_err(|e| e.to_string())?;
    say(&json!({
        "claude_port": server.port(),
        "token": server.token(),
        "mobile_port": mobile.port(),
        "host": args.host,
    }));
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        say(&answer(&mobile, &line));
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mobile_rig: {e}");
        std::process::exit(1);
    }
}
