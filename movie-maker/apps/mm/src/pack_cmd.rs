//! `mm pack`: パック（docs/design/13-vlog-pack.md）の検査、マクロの一覧、マクロの展開と適用、書体の取得。
//! 適用は `mm op --txn` と同じ経路を通るので、アプリが書き手なら HTTP で転送し、取り消しは `mm undo`。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use mm_pack::{
    check, copy_fonts, expand, expand_for, font_asset_ops, font_files, CachedFont, Curl,
    FetchReport, FontCache, Pack,
};
use mm_runtime::{Backend, Engine, RuntimeConfig};
use mm_types::{MmError, Result};

use crate::commands::{apply_txn, open_reader};

/// `mm pack`。上の段の登録は `main.rs` の 1 行。
#[derive(Subcommand)]
pub enum Cmd {
    /// パック（vlog など）の検査、マクロの一覧、マクロを展開して適用する。
    Pack {
        #[command(subcommand)]
        command: PackCommand,
    },
}

impl Cmd {
    /// サブコマンドを実行する。
    pub fn run(self) -> Result<()> {
        match self {
            Self::Pack { command } => run(command),
        }
    }
}

/// `mm pack` のサブコマンド。
#[derive(Subcommand)]
pub enum PackCommand {
    /// パックの形式を検査する。マクロを例の引数で展開し、テンプレートを適用した空のプロジェクトに適用してみる。
    Check { pack: PathBuf },
    /// テンプレートとマクロの一覧（引数の型と既定）。
    List {
        pack: PathBuf,
        /// 1 行に 1 件の JSON で出す。
        #[arg(long)]
        json: bool,
    },
    /// マクロを展開して、適用せずに `Transaction` の JSON を出す（`mm proposal add --txn` に渡せる）。
    Expand {
        pack: PathBuf,
        name: String,
        /// 引数（`--var text=こんにちは`）。
        #[arg(long = "var", value_parser = parse_var)]
        vars: Vec<(String, String)>,
    },
    /// パックの書体の目録（`fonts/fonts.json`）の書体を配布元から取り、ハッシュを照らしてユーザーのキャッシュに置く。
    /// `apply` も足りない書体を取るので、先に取っておきたいとき（オフラインで作業する前など）に使う。
    FetchFonts { pack: PathBuf },
    /// マクロを展開して 1 つのトランザクションとして適用する。
    /// `mm pack apply <dir> packs/vlog caption --var id=c1 --var text=やあ --var at=1.5`
    Apply {
        dir: PathBuf,
        pack: PathBuf,
        name: String,
        /// 引数（`--var text=こんにちは`）。
        #[arg(long = "var", value_parser = parse_var)]
        vars: Vec<(String, String)>,
        /// 元にしたプロジェクトのデータのバージョン（`mm op --base` と同じ）。
        #[arg(long)]
        base: Option<u64>,
    },
}

/// `mm pack` を行う。
pub fn run(command: PackCommand) -> Result<()> {
    match command {
        PackCommand::Check { pack } => run_check(&pack),
        PackCommand::List { pack, json } => list(&pack, json),
        PackCommand::FetchFonts { pack } => fetch_fonts(&pack),
        PackCommand::Expand { pack, name, vars } => {
            let pack = Pack::load(&pack)?;
            let engine = text_engine(&ensure_fonts(&pack))?;
            let args = vars.into_iter().collect();
            let txn = expand(&pack, &name, &args, &*engine.services().text_shaper)?;
            println!("{}", serde_json::to_string_pretty(&txn)?);
            Ok(())
        }
        PackCommand::Apply {
            dir,
            pack,
            name,
            vars,
            base,
        } => {
            let args: BTreeMap<String, String> = vars.into_iter().collect();
            let pack = Pack::load(&pack)?;
            let fonts = ensure_fonts(&pack);
            let engine = text_engine(&fonts)?;
            let shaper = &*engine.services().text_shaper;
            // 部品を置くコンポジションの画面の大きさ（ショートなど）をプロジェクトから引く。書き手がアプリでも読める
            let project = open_reader(&dir)?.project().clone();
            let mut txn = expand_for(&pack, &name, &args, shaper, &project)?;
            txn.ops.splice(0..0, import_fonts(&pack, &fonts, &dir)?);
            txn.base_version = base;
            apply_txn(&dir, txn)
        }
    }
}

/// プレートの大きさを測るための、文字だけ実際の実装（フォントを読む CPU の実装）を選んだエンジン。
/// パックの書体を読み込むので、描くときと同じ字の幅になる。
fn text_engine(fonts: &[CachedFont]) -> Result<Engine> {
    let engine = mm_runtime::assemble(&RuntimeConfig {
        text: Backend::Cpu,
        ..RuntimeConfig::sample()
    })?;
    engine
        .services()
        .text_shaper
        .use_fonts(&font_files(fonts))?;
    Ok(engine)
}

/// `mm pack fetch-fonts`。取れなかった書体があれば誤りで終わる（取れたものはキャッシュに残る）。
fn fetch_fonts(dir: &Path) -> Result<()> {
    let pack = Pack::load(dir)?;
    let cache = FontCache::user()?;
    let report = cache.fetch(&pack.fonts, &Curl);
    println!(
        "書体: 取った {}、キャッシュにあった {}（{}）",
        report.fetched,
        report.cached,
        cache.dir().display()
    );
    print_failed(&report);
    if report.failed.is_empty() {
        return Ok(());
    }
    Err(MmError::NotFound(format!(
        "書体 {} 件を取れなかった",
        report.failed.len()
    )))
}

/// キャッシュにある書体を返す。足りなければ取る。取れなくても止めずに、OS の書体で描くと伝えて進む。
fn ensure_fonts(pack: &Pack) -> Vec<CachedFont> {
    if pack.fonts.is_empty() {
        return Vec::new();
    }
    let cache = match FontCache::user() {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("書体を使わずに進む（OS の書体で描く）: {e}");
            return Vec::new();
        }
    };
    let have = cache.resolve(&pack.fonts);
    if have.len() == pack.fonts.len() {
        return have;
    }
    eprintln!("書体を配布元から取る（{}）", cache.dir().display());
    let report = cache.fetch(&pack.fonts, &Curl);
    if !report.failed.is_empty() {
        print_failed(&report);
        eprintln!("取れなかった書体は OS の書体で描く。`mm pack fetch-fonts` で取り直せる");
    }
    cache.resolve(&pack.fonts)
}

fn print_failed(report: &FetchReport) {
    for (file, reason) in &report.failed {
        eprintln!("  {file}: {reason}");
    }
}

/// 書体をプロジェクトのフォルダーの `fonts/` に写し、まだ素材に無いものを足す操作を返す。
/// JSON のプロジェクトはその JSON のあるフォルダーに写す（素材の相対パスの基準と同じ）。
fn import_fonts(pack: &Pack, fonts: &[CachedFont], dir: &Path) -> Result<Vec<mm_ops::Op>> {
    if fonts.is_empty() {
        return Ok(Vec::new());
    }
    let project = mm_store::load_project(dir)?;
    let base = if dir.is_dir() {
        dir
    } else {
        dir.parent().unwrap_or(Path::new("."))
    };
    copy_fonts(fonts, &pack.dir, base)?;
    Ok(font_asset_ops(fonts, &project))
}

fn run_check(dir: &Path) -> Result<()> {
    let pack = Pack::load(dir)?;
    let report = check(&pack);
    if report.ok() {
        println!(
            "{} {}: 通った（テンプレートとマクロ {} 個を例で適用した）",
            pack.manifest.name, pack.manifest.version, report.applied
        );
        return Ok(());
    }
    for e in &report.errors {
        eprintln!("  {e}");
    }
    Err(MmError::InvalidArgument(format!(
        "パック {} の検査で {} 件の誤り",
        pack.manifest.name,
        report.errors.len()
    )))
}

fn list(dir: &Path, json: bool) -> Result<()> {
    let pack = Pack::load(dir)?;
    for def in &pack.macros {
        let summary = def.summary();
        if json {
            println!("{}", serde_json::to_string(&summary)?);
            continue;
        }
        let params: Vec<String> = summary
            .params
            .iter()
            .map(|(name, p)| match (&p.default, p.optional) {
                (Some(d), _) => format!("{name}={d}"),
                (None, true) => format!("[{name}]"),
                (None, false) => name.clone(),
            })
            .collect();
        println!("{}\t{}\t{}", def.name, def.title, params.join(" "));
    }
    Ok(())
}

/// `名前=値` を組にする。
fn parse_var(raw: &str) -> std::result::Result<(String, String), String> {
    raw.split_once('=')
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| format!("引数は 名前=値 の形: {raw}"))
}
