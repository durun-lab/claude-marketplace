//! 囲いの検査。取り込みの前に、コンパイル、変えたクレートと依存の逆引きのテスト、WebView の型、性能、書き出しの一致を確かめる。
//! [`Checks`] を差し替えられる層にし、実際の実装は [`CargoChecks`]（cargo、pnpm、`mm perf`、`mm export` を回の worktree で実行する）。
//! 領域の検査（areas.toml）は差し替えず、回が必ず通す（round.rs）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use mm_types::{MmError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::proc::{run, Ran};

const TAIL_LINES: usize = 20;
const PERCENT: f64 = 100.0;

/// 回の前に取った基準。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    /// `mm perf run` の結果のフォルダー。
    pub perf_dir: Option<PathBuf>,
    /// フィクスチャのプロジェクトの `mm export` のハッシュ。
    pub export_hash: Option<String>,
}

/// 1 つの検査の判定。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// 通った。
    Pass,
    /// 失敗した。取り込まず、要望は未対応に残る。
    Fail,
    /// 人が見る。PR までで止める。
    Hold,
}

/// 1 つの検査の結果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    /// 検査の名前。
    pub name: String,
    /// 判定。
    pub verdict: Verdict,
    /// 返事に載せる中身（失敗した項目と数など）。
    pub detail: String,
}

impl CheckOutcome {
    /// 結果を作る。
    pub fn new(name: &str, verdict: Verdict, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            verdict,
            detail: detail.into(),
        }
    }
}

/// 検査に渡す回の事情。
pub struct CheckCx<'a> {
    /// 回の worktree の movie-maker。
    pub root: &'a Path,
    /// 回のフォルダー（性能の結果、書き出しの一時フォルダー）。
    pub run_dir: &'a Path,
    /// 変えたファイル（`root` からの相対）。
    pub changed: &'a [String],
    /// `mm perf compare` の閾値（割合）。
    pub perf_threshold: f64,
}

/// 囲いの検査。
pub trait Checks: Send + Sync {
    /// 回の前に基準を取る（実装を任せる前の worktree で呼ぶ）。
    fn baseline(&self, cx: &CheckCx<'_>) -> Result<Baseline>;
    /// 取り込みの前に検査する。失敗したら後の重い検査は省いてよい。
    fn verify(&self, cx: &CheckCx<'_>, baseline: &Baseline) -> Vec<CheckOutcome>;
}

/// cargo と pnpm と `mm` で検査する実際の実装。
#[derive(Clone, Debug)]
pub struct CargoChecks {
    /// 回の `CARGO_TARGET_DIR`。
    pub target_dir: PathBuf,
    /// `mm perf run` のシナリオ。`None` なら性能を比べない。
    pub perf_scenario: Option<String>,
    /// `mm perf run --repeat`。
    pub perf_repeat: u32,
    /// `mm export` のハッシュを比べるフィクスチャのプロジェクト（`root` からの相対の JSON）。
    pub export_fixture: Option<PathBuf>,
    /// `mm` を実行する cargo の引数。
    pub mm: Vec<String>,
    /// WebView の型の検査のコマンド。
    pub webview: Vec<String>,
}

impl CargoChecks {
    /// 設計書の既定（`mm perf run all` の 5 回、`fixtures/projects/stage0`）で作る。
    pub fn new(target_dir: impl Into<PathBuf>) -> Self {
        Self {
            target_dir: target_dir.into(),
            perf_scenario: Some("all".into()),
            perf_repeat: 5,
            export_fixture: Some(PathBuf::from("fixtures/projects/stage0/project.json")),
            mm: ["run", "--release", "-q", "-p", "mm", "--"]
                .map(str::to_owned)
                .to_vec(),
            webview: ["pnpm", "-C", "ui", "typecheck"]
                .map(str::to_owned)
                .to_vec(),
        }
    }

    fn cargo(&self, root: &Path) -> Command {
        let mut c = mm_types::process::command("cargo");
        c.current_dir(root)
            .env("CARGO_TARGET_DIR", &self.target_dir)
            .env("CARGO_INCREMENTAL", "0");
        c
    }

    fn mm(&self, root: &Path, args: &[&str]) -> Result<Ran> {
        run(self.cargo(root).args(&self.mm).args(args))
    }

    fn perf_run(&self, cx: &CheckCx<'_>, scenario: &str, side: &str) -> Result<PathBuf> {
        let dir = cx.run_dir.join("perf").join(side);
        let repeat = self.perf_repeat.to_string();
        let out = dir.to_string_lossy().into_owned();
        let ran = self.mm(
            cx.root,
            &[
                "perf",
                "run",
                scenario,
                "--repeat",
                &repeat,
                "--out-dir",
                &out,
            ],
        )?;
        if !ran.ok {
            return Err(MmError::Internal(format!(
                "mm perf run {scenario}: {}",
                ran.tail(TAIL_LINES)
            )));
        }
        Ok(dir)
    }

    fn export_hash(&self, cx: &CheckCx<'_>, fixture: &Path, side: &str) -> Result<String> {
        let dir = cx.run_dir.join(format!("export-{side}"));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        let d = dir.to_string_lossy().into_owned();
        let src = cx.root.join(fixture).to_string_lossy().into_owned();
        let imported = self.mm(cx.root, &["import", &d, &src])?;
        if !imported.ok {
            return Err(MmError::Internal(format!(
                "mm import: {}",
                imported.tail(TAIL_LINES)
            )));
        }
        let exported = self.mm(cx.root, &["export", &d])?;
        parse_export_hash(&exported.stdout)
            .filter(|_| exported.ok)
            .ok_or_else(|| MmError::Internal(format!("mm export: {}", exported.tail(TAIL_LINES))))
    }

    fn cargo_step(
        &self,
        cx: &CheckCx<'_>,
        name: &str,
        args: &[&str],
        crates: &[String],
    ) -> CheckOutcome {
        let mut cmd = self.cargo(cx.root);
        cmd.args(args);
        for c in crates {
            cmd.args(["-p", c]);
        }
        outcome_of(name, run(&mut cmd))
    }

    fn webview_step(&self, cx: &CheckCx<'_>) -> CheckOutcome {
        let Some((prog, args)) = self.webview.split_first() else {
            return CheckOutcome::new("webview", Verdict::Pass, "検査のコマンドが無い");
        };
        outcome_of(
            "webview",
            run(mm_types::process::command(prog)
                .args(args)
                .current_dir(cx.root)),
        )
    }

    fn perf_step(&self, cx: &CheckCx<'_>, before: &Path) -> CheckOutcome {
        let Some(scenario) = &self.perf_scenario else {
            return CheckOutcome::new("perf", Verdict::Pass, "比べない");
        };
        let after = match self.perf_run(cx, scenario, "after") {
            Ok(d) => d,
            Err(e) => return CheckOutcome::new("perf", Verdict::Fail, e.to_string()),
        };
        let threshold = format!("{}%", cx.perf_threshold * PERCENT);
        let (b, a) = (before.to_string_lossy(), after.to_string_lossy());
        match self.mm(
            cx.root,
            &["perf", "compare", &b, &a, "--threshold", &threshold],
        ) {
            Ok(r) if r.ok => {
                CheckOutcome::new("perf", Verdict::Pass, format!("閾値 {threshold} の中"))
            }
            Ok(r) => CheckOutcome::new("perf", Verdict::Fail, regressions(&r)),
            Err(e) => CheckOutcome::new("perf", Verdict::Fail, e.to_string()),
        }
    }

    fn export_step(&self, cx: &CheckCx<'_>, before: &str) -> CheckOutcome {
        let Some(fixture) = &self.export_fixture else {
            return CheckOutcome::new("export", Verdict::Pass, "比べない");
        };
        match self.export_hash(cx, fixture, "after") {
            Ok(h) if h == before => {
                CheckOutcome::new("export", Verdict::Pass, format!("ハッシュ {h}"))
            }
            Ok(h) => CheckOutcome::new(
                "export",
                Verdict::Hold,
                format!(
                    "{} の書き出しのハッシュが変わった（前 {before}、後 {h}）",
                    fixture.display()
                ),
            ),
            Err(e) => CheckOutcome::new("export", Verdict::Fail, e.to_string()),
        }
    }

    fn affected(&self, cx: &CheckCx<'_>) -> Result<Vec<String>> {
        let ran =
            run(self
                .cargo(cx.root)
                .args(["metadata", "--format-version", "1", "--no-deps"]))?;
        if !ran.ok {
            return Err(MmError::Internal(format!(
                "cargo metadata: {}",
                ran.tail(TAIL_LINES)
            )));
        }
        Ok(affected_packages(
            &serde_json::from_str(&ran.stdout)?,
            cx.changed,
        ))
    }
}

impl Checks for CargoChecks {
    fn baseline(&self, cx: &CheckCx<'_>) -> Result<Baseline> {
        let perf_dir = match &self.perf_scenario {
            Some(s) => Some(self.perf_run(cx, s, "before")?),
            None => None,
        };
        let export_hash = match &self.export_fixture {
            Some(f) => Some(self.export_hash(cx, f, "before")?),
            None => None,
        };
        Ok(Baseline {
            perf_dir,
            export_hash,
        })
    }

    fn verify(&self, cx: &CheckCx<'_>, baseline: &Baseline) -> Vec<CheckOutcome> {
        let owned = match self.affected(cx) {
            Ok(c) => c,
            Err(e) => {
                return vec![CheckOutcome::new(
                    "cargo metadata",
                    Verdict::Fail,
                    e.to_string(),
                )]
            }
        };
        let crates: &[String] = &owned;
        let mut steps: Vec<Box<dyn Fn() -> CheckOutcome + '_>> = Vec::new();
        if !crates.is_empty() {
            let check = ["check", "--all-targets"];
            steps.push(Box::new(move || {
                self.cargo_step(cx, "cargo check", &check, crates)
            }));
            let nextest = ["nextest", "run", "--no-tests=pass"];
            steps.push(Box::new(move || {
                self.cargo_step(cx, "cargo nextest", &nextest, crates)
            }));
        }
        if cx.changed.iter().any(|f| f.starts_with("ui/")) {
            steps.push(Box::new(move || self.webview_step(cx)));
        }
        if let Some(before) = &baseline.perf_dir {
            steps.push(Box::new(move || self.perf_step(cx, before)));
        }
        if let Some(before) = &baseline.export_hash {
            steps.push(Box::new(move || self.export_step(cx, before)));
        }
        let mut out = Vec::new();
        for step in steps {
            let o = step();
            let failed = o.verdict == Verdict::Fail;
            out.push(o);
            if failed {
                break;
            }
        }
        out
    }
}

fn outcome_of(name: &str, ran: Result<Ran>) -> CheckOutcome {
    match ran {
        Ok(r) if r.ok => CheckOutcome::new(name, Verdict::Pass, ""),
        Ok(r) => CheckOutcome::new(name, Verdict::Fail, r.tail(TAIL_LINES)),
        Err(e) => CheckOutcome::new(name, Verdict::Fail, e.to_string()),
    }
}

/// `mm perf compare` の出力から、回帰の行と最後の「回帰:」の行を拾う。返事に項目と数を書く。
pub fn regressions(ran: &Ran) -> String {
    let lines: Vec<&str> = ran
        .stdout
        .lines()
        .chain(ran.stderr.lines())
        .filter(|l| l.trim_end().ends_with("回帰") || l.starts_with("回帰:"))
        .collect();
    if lines.is_empty() {
        ran.tail(TAIL_LINES)
    } else {
        lines.join("\n")
    }
}

/// `mm export` の出力の「ハッシュ <値>」を読む。
pub fn parse_export_hash(stdout: &str) -> Option<String> {
    let (_, rest) = stdout.rsplit_once("ハッシュ ")?;
    rest.split_whitespace().next().map(str::to_owned)
}

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_owned()
}

/// 変えたファイルを持つクレートと、それに依存するクレート（逆引きの閉包）。`changed` は workspace のルートからの相対。
pub fn affected_packages(metadata: &Value, changed: &[String]) -> Vec<String> {
    let root = norm(metadata["workspace_root"].as_str().unwrap_or(""));
    let empty = Vec::new();
    let pkgs = metadata["packages"].as_array().unwrap_or(&empty);
    let mut dirs: Vec<(String, String)> = Vec::new();
    let mut rdeps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in pkgs {
        let name = p["name"].as_str().unwrap_or("").to_owned();
        let manifest = norm(p["manifest_path"].as_str().unwrap_or(""));
        let dir = manifest.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let rel = dir
            .strip_prefix(&root)
            .unwrap_or(dir)
            .trim_start_matches('/')
            .to_owned();
        dirs.push((rel, name.clone()));
        for d in p["dependencies"].as_array().unwrap_or(&empty) {
            if d["path"].is_string() {
                let dep = d["name"].as_str().unwrap_or("").to_owned();
                rdeps.entry(dep).or_default().push(name.clone());
            }
        }
    }
    // いちばん深いクレートのフォルダーに割り当てる（ルートのパッケージと入れ子を分ける）。
    dirs.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
    let mut hit: BTreeSet<String> = BTreeSet::new();
    for f in changed {
        let owner = dirs
            .iter()
            .find(|(d, _)| d.is_empty() || f.starts_with(&format!("{d}/")));
        hit.extend(owner.map(|(_, n)| n.clone()));
    }
    let mut stack: Vec<String> = hit.iter().cloned().collect();
    while let Some(n) = stack.pop() {
        for r in rdeps.get(&n).into_iter().flatten() {
            if hit.insert(r.clone()) {
                stack.push(r.clone());
            }
        }
    }
    hit.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_hash_is_read_from_last_line() {
        let out = "x/versions: 3 ファイル（3 件を書いた）、ハッシュ abc123\n";
        assert_eq!(parse_export_hash(out).as_deref(), Some("abc123"));
        assert_eq!(parse_export_hash("nothing"), None);
    }

    #[test]
    fn affected_includes_reverse_dependencies() {
        let meta = serde_json::json!({
            "workspace_root": "E:\\w",
            "packages": [
                { "name": "mm-types", "manifest_path": "E:\\w\\crates\\mm-types\\Cargo.toml", "dependencies": [] },
                { "name": "mm-ops", "manifest_path": "E:\\w\\crates\\mm-ops\\Cargo.toml",
                  "dependencies": [{ "name": "mm-types", "path": "E:\\w\\crates\\mm-types" }] },
                { "name": "mm", "manifest_path": "E:\\w\\apps\\mm\\Cargo.toml",
                  "dependencies": [{ "name": "mm-ops", "path": "E:\\w\\crates\\mm-ops" }, { "name": "serde" }] },
                { "name": "mm-pack", "manifest_path": "E:\\w\\crates\\mm-pack\\Cargo.toml", "dependencies": [] }
            ]
        });
        let got = affected_packages(
            &meta,
            &["crates/mm-types/src/lib.rs".into(), "docs/a.md".into()],
        );
        assert_eq!(got, vec!["mm", "mm-ops", "mm-types"]);
    }
}
