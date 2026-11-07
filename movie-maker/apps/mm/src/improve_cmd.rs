//! `mm improve`: 自動の改善（mm-improve）を試す。要望の起票と一覧、状態、設定、回をこの場で回す、すぐ止める、areas.toml の検査。
//! アプリと同じ置き場所（`--data-dir`、既定は `%LOCALAPPDATA%/movie-maker/improve`）を読み書きする。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Subcommand;
use mm_improve::{
    git, Areas, CargoChecks, ClaudeCli, FeedbackItem, GhForge, ImproveConfig, ImproveVerdict,
    Improver, ListFilter, NewFeedback, SettingsPatch, Step,
};
use mm_types::Result;

use crate::compare::parse_threshold;

/// `mm improve`。上の段の登録は `main.rs` の 1 行。
#[derive(Subcommand)]
pub enum Cmd {
    /// 自動の改善。要望をためて、別の worktree で claude -p に直させ、検査を通ったものを取り込む。
    Improve {
        /// 要望と回の状態の置き場所。
        #[arg(long, global = true)]
        data_dir: Option<PathBuf>,
        /// 取り込む先のブランチ。人の main のチェックアウトの WIP を巻き込まないよう、既定は専用のブランチ。
        /// 無ければ `--upstream` から切る。
        #[arg(long, global = true, default_value = mm_improve::DEFAULT_MAINLINE)]
        mainline: String,
        /// 取り込む先を切る元で、PR までで止めるときの PR の向き先。
        #[arg(long, global = true, default_value = mm_improve::DEFAULT_UPSTREAM)]
        upstream: String,
        #[command(subcommand)]
        command: ImproveCommand,
    },
}

/// `mm improve` のサブコマンド。
#[derive(Subcommand)]
pub enum ImproveCommand {
    /// 状態（ON / OFF、動いている回、未対応の数、待機の理由、最後の回）。
    Status {
        /// JSON で出す。
        #[arg(long)]
        json: bool,
    },
    /// 要望を起票する。
    Submit {
        /// 要望の本文。
        text: Vec<String>,
        /// 送ったときの状態の JSON のファイル。
        #[arg(long)]
        context: Option<PathBuf>,
    },
    /// 要望の一覧（新しい順）。
    List {
        /// 未対応だけ。
        #[arg(long)]
        pending: bool,
        /// 1 行に 1 件の JSON で出す。
        #[arg(long)]
        json: bool,
    },
    /// 要望を 1 件、返事と一緒に出す。
    Show { id: String },
    /// 設定を変える。
    Set {
        /// ON にする。
        #[arg(long, conflicts_with = "off")]
        on: bool,
        /// OFF にする（動いている回は終えてから止まる。すぐ止めるのは stop）。
        #[arg(long)]
        off: bool,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        /// 回の予算（ドル）。0 で上限なし。
        #[arg(long)]
        budget_usd: Option<f64>,
        /// 回の時間の上限（分）。
        #[arg(long)]
        wall_minutes: Option<u32>,
        /// `mm perf compare` の閾値（`5%` か `0.05`）。
        #[arg(long, value_parser = parse_threshold)]
        threshold: Option<f64>,
    },
    /// 回を 1 つこの場で回す（ON で未対応があるとき）。使用制限で止まった回は、明けていれば続きから。
    Run {
        /// `mm perf run` のシナリオ。`none` で性能を比べない。
        #[arg(long, default_value = "all")]
        perf: String,
        /// `mm export` のハッシュを比べない。
        #[arg(long)]
        no_export: bool,
    },
    /// すぐ止める。OFF にし、動いている claude -p を止める。
    Stop,
    /// 変えたファイルを docs/areas.toml に突き合わせる（共有のファイル、領域の外、自動の改善の取り込みの判定）。
    Areas {
        /// 照らす領域。省くと共有のファイルと取り込みの判定だけ。
        #[arg(long)]
        area: Option<String>,
        /// 比べる元。
        #[arg(long, default_value = "origin/main")]
        base: String,
    },
}

impl Cmd {
    /// サブコマンドを実行する。
    pub fn run(self) -> Result<()> {
        let Self::Improve {
            data_dir,
            mainline,
            upstream,
            command,
        } = self;
        let data_dir = match data_dir {
            Some(d) => d,
            None => ImproveConfig::default_data_dir()?,
        };
        let cfg = ImproveConfig::discover(data_dir, &std::env::current_dir()?)?
            .with_mainline(mainline)
            .with_upstream(upstream);
        run(cfg, command)
    }
}

fn run(cfg: ImproveConfig, command: ImproveCommand) -> Result<()> {
    let (perf, export) = match &command {
        ImproveCommand::Run { perf, no_export } => (perf.clone(), !*no_export),
        _ => ("none".to_owned(), false),
    };
    let imp = improver(cfg, &perf, export);
    match command {
        ImproveCommand::Status { json } => status(&imp, json),
        ImproveCommand::Submit { text, context } => {
            submit(&imp, &text.join(" "), context.as_deref())
        }
        ImproveCommand::List { pending, json } => list(&imp, pending, json),
        ImproveCommand::Show { id } => print_json(serde_json::to_value(imp.get(&id)?)),
        ImproveCommand::Set {
            on,
            off,
            model,
            effort,
            budget_usd,
            wall_minutes,
            threshold,
        } => {
            let enabled = (on || off).then_some(on);
            let patch = SettingsPatch {
                enabled,
                model,
                effort,
                budget_usd,
                wall_minutes,
                perf_threshold: threshold,
            };
            print_json(serde_json::to_value(imp.update_settings(&patch)?))
        }
        ImproveCommand::Run { .. } => run_once(&imp),
        ImproveCommand::Stop => print_json(serde_json::to_value(imp.stop()?)),
        ImproveCommand::Areas { area, base } => areas(imp.config(), area.as_deref(), &base),
    }
}

/// 回の `CARGO_TARGET_DIR`。呼んだ人の指定があればそれ、なければ置き場所の `target/`。
fn target_dir(cfg: &ImproveConfig) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| cfg.data_dir.join("target"))
}

fn improver(cfg: ImproveConfig, perf: &str, export: bool) -> Improver {
    let target = target_dir(&cfg);
    let mut checks = CargoChecks::new(&target);
    checks.perf_scenario = (perf != "none").then(|| perf.to_owned());
    if !export {
        checks.export_fixture = None;
    }
    let agent = ClaudeCli {
        env: vec![
            (
                "CARGO_TARGET_DIR".into(),
                target.to_string_lossy().into_owned(),
            ),
            ("CARGO_INCREMENTAL".into(), "0".into()),
        ],
        ..ClaudeCli::default()
    };
    Improver::new(cfg, Arc::new(agent), Arc::new(checks), Arc::new(GhForge))
}

/// 整形した JSON を出す。serde に直接は依存しないので、値は `serde_json::to_value` で受け取る。
fn print_json(v: serde_json::Result<serde_json::Value>) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&v?)?);
    Ok(())
}

fn status(imp: &Improver, json: bool) -> Result<()> {
    let s = imp.status()?;
    if json {
        return print_json(serde_json::to_value(&s));
    }
    println!(
        "{}、未対応 {} 件",
        if s.settings.enabled { "ON" } else { "OFF" },
        s.pending
    );
    if let Some(r) = &s.run {
        let limit = r
            .limit_resets_at
            .map(|t| format!("、使用制限が明けるのを待っている（{t}）"))
            .unwrap_or_default();
        println!(
            "回 {}: 要望 {} 件、{:?}{limit}",
            r.id,
            r.items.len(),
            r.phase
        );
    }
    if let Some(w) = &s.waiting {
        println!("待機: {w}");
    }
    if let Some(l) = &s.last {
        println!("最後の回 {}: {:?}", l.run_id, l.outcome);
    }
    Ok(())
}

fn submit(imp: &Improver, text: &str, context: Option<&Path>) -> Result<()> {
    let context = match context {
        Some(p) => serde_json::from_slice(&std::fs::read(p)?)?,
        None => serde_json::json!({ "from": "mm improve submit" }),
    };
    let item = imp.submit(NewFeedback {
        text: text.into(),
        targets: Vec::new(),
        context,
    })?;
    println!("{}", item.id);
    Ok(())
}

fn list(imp: &Improver, pending: bool, json: bool) -> Result<()> {
    let filter = if pending {
        ListFilter::Pending
    } else {
        ListFilter::All
    };
    for item in imp.list(filter)? {
        if json {
            println!("{}", serde_json::to_string(&item)?);
        } else {
            println!("{}", line_of(&item));
        }
    }
    Ok(())
}

fn line_of(item: &FeedbackItem) -> String {
    let text: String = item
        .text
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(60)
        .collect();
    let reply = item
        .replies
        .last()
        .map(|r| format!(" → {}", r.text.lines().next().unwrap_or("")))
        .unwrap_or_default();
    format!(
        "{} {:?} {:?} {text}{reply}",
        item.id, item.status, item.kind
    )
}

fn run_once(imp: &Improver) -> Result<()> {
    match imp.run_once()? {
        Step::Idle(why) => println!("回を起こさなかった: {why}"),
        Step::Busy => println!("ほかで回が動いている"),
        Step::Limited { resets_at } => {
            println!("使用制限で止まっている。{resets_at} に明けたら mm improve run で続ける")
        }
        Step::Finished(report) => print_json(serde_json::to_value(&report))?,
    }
    Ok(())
}

fn areas(cfg: &ImproveConfig, area: Option<&str>, base: &str) -> Result<()> {
    let root = cfg.repo.join(&cfg.subdir);
    let parsed = Areas::parse(&std::fs::read_to_string(root.join("docs/areas.toml"))?)?;
    // 元の側で進んだ変更を混ぜないよう、分かれた所（merge-base）から作業ツリーまでを比べる。
    let fork = git(&root, &["merge-base", base, "HEAD"])?;
    let mut files: Vec<String> = git(&root, &["diff", "--name-only", "--relative", &fork])?
        .lines()
        .map(str::to_owned)
        .collect();
    files.extend(
        git(&root, &["ls-files", "--others", "--exclude-standard"])?
            .lines()
            .map(str::to_owned),
    );
    files.sort();
    files.dedup();
    println!(
        "共有のファイル: {}",
        parsed.shared_touched(&files).join(", ")
    );
    if let Some(a) = area {
        println!(
            "領域 {a} の外: {}",
            parsed.check_area(a, &files)?.outside.join(", ")
        );
    }
    match parsed.improve_verdict(&files) {
        ImproveVerdict::Merge => println!("自動の改善なら取り込める"),
        ImproveVerdict::Hold(held) => {
            println!("自動の改善なら PR までで止める: {}", held.join(", "))
        }
    }
    Ok(())
}
