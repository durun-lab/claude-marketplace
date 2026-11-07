//! フィクスチャのリポジトリ（一時フォルダーの git）と、差し替えの偽物。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use mm_improve::{
    git, AgentEnd, AgentRequest, Baseline, CheckCx, CheckOutcome, Checks, Forge, ImproveConfig,
    Improver, PullRequest, SettingsPatch,
};
use mm_types::Result;

/// フィクスチャの areas.toml。`src/` は取り込み、`core/` は人の承認、`src/golden/` は取り込みの中でも人が見る。
const AREAS: &str = "[improve]\nmerge = [\"src/\"]\nhold = [\"src/golden/\"]\n";

/// 一時フォルダーの git。根の下の `mm/` を movie-maker に見立てる。
pub struct Fixture {
    _dir: tempfile::TempDir,
    pub repo: PathBuf,
    pub data: PathBuf,
}

pub fn run_git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let mm = repo.join("mm");
        std::fs::create_dir_all(mm.join("docs")).unwrap();
        std::fs::create_dir_all(mm.join("src")).unwrap();
        std::fs::create_dir_all(mm.join("core")).unwrap();
        std::fs::write(mm.join("docs/areas.toml"), AREAS).unwrap();
        std::fs::write(mm.join("src/greet.txt"), "Helo, world\n").unwrap();
        std::fs::write(mm.join("core/engine.txt"), "engine\n").unwrap();
        run_git(&repo, &["init", "-q", "-b", "main"]);
        run_git(&repo, &["config", "user.name", "mm-improve test"]);
        run_git(&repo, &["config", "user.email", "test@example.invalid"]);
        run_git(&repo, &["config", "core.autocrlf", "false"]);
        run_git(&repo, &["add", "-A"]);
        run_git(&repo, &["commit", "-q", "-m", "init"]);
        // PR の土台の origin/main を引けるよう、遠隔に見立てた裸のリポジトリを置く。
        let origin = dir.path().join("origin.git");
        run_git(
            dir.path(),
            &["init", "-q", "--bare", "-b", "main", "origin.git"],
        );
        run_git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        run_git(&repo, &["push", "-q", "origin", "main"]);
        let data = dir.path().join("data");
        Self {
            _dir: dir,
            repo,
            data,
        }
    }

    pub fn config(&self) -> ImproveConfig {
        ImproveConfig::new(&self.data, &self.repo).with_subdir("mm")
    }

    /// 取り込む先（既定の `local/movie-maker`）のファイルの中身。
    pub fn main_file(&self, path: &str) -> String {
        self.file_at(mm_improve::DEFAULT_MAINLINE, path)
    }

    /// ブランチ `rev` のファイルの中身。
    pub fn file_at(&self, rev: &str, path: &str) -> String {
        git(&self.repo, &["show", &format!("{rev}:mm/{path}")]).unwrap()
    }

    /// 取り込み先だけにあるコミット（main と origin/main には無い先行分）を作る。
    pub fn local_commit(&self, path: &str, body: &str) {
        let mainline = mm_improve::DEFAULT_MAINLINE;
        run_git(&self.repo, &["checkout", "-q", "-b", mainline]);
        std::fs::write(self.repo.join("mm").join(path), body).unwrap();
        run_git(&self.repo, &["add", "-A"]);
        run_git(&self.repo, &["commit", "-q", "-m", "local only"]);
        run_git(&self.repo, &["checkout", "-q", "main"]);
    }

    /// `from` から `to` までに変えたファイル。
    pub fn diff_names(&self, from: &str, to: &str) -> Vec<String> {
        let out = git(&self.repo, &["diff", "--name-only", from, to]).unwrap();
        out.lines().map(str::to_owned).collect()
    }

    pub fn branch_exists(&self, branch: &str) -> bool {
        git(&self.repo, &["rev-parse", "--verify", "--quiet", branch]).is_ok()
    }

    pub fn worktrees(&self) -> usize {
        git(&self.repo, &["worktree", "list", "--porcelain"])
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count()
    }

    /// 偽物で組んで ON にする。
    pub fn improver(
        &self,
        agent: Arc<FakeAgent>,
        checks: Arc<FakeChecks>,
        forge: Arc<FakeForge>,
    ) -> Improver {
        let imp = Improver::new(self.config(), agent, checks, forge);
        imp.update_settings(&SettingsPatch {
            enabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        imp
    }
}

/// 偽物の 1 回の振る舞い。
pub enum Script {
    /// ファイルを書いて、渡された要望の全部に返事をして終える。
    Write(Vec<(&'static str, &'static str)>),
    /// 何も書かずに、文だけで終える（印なし）。
    Say(&'static str),
    /// 使用制限で止まる。
    Limit(u64),
    /// 任意の振る舞い。引数は呼び出しと 1 回目の頼む文（要望の ID が載っている）。
    Run(fn(&AgentRequest<'_>, &str) -> AgentEnd),
}

/// 1 回目の頼む文に載った要望の全部に返事をした最後の発言。
pub fn done_for(first_prompt: &str) -> AgentEnd {
    let lines: Vec<String> = ids_in(first_prompt)
        .iter()
        .map(|id| format!("- {id}: 直しました"))
        .collect();
    AgentEnd::Finished {
        last_message: format!("直した。\n\n対応した要望:\n{}", lines.join("\n")),
    }
}

/// 決まった差分を書く `claude -p` の偽物。
#[derive(Default)]
pub struct FakeAgent {
    script: Mutex<VecDeque<Script>>,
    /// 呼ばれたときの（続けたか、頼む文）。
    pub calls: Mutex<Vec<(bool, String)>>,
    /// 呼ばれたときに渡された予算（回の残り）。
    pub budgets: Mutex<Vec<Option<f64>>>,
}

impl FakeAgent {
    pub fn new(script: Vec<Script>) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script.into()),
            ..Default::default()
        })
    }

    pub fn calls(&self) -> Vec<(bool, String)> {
        self.calls.lock().unwrap().clone()
    }
}

/// 頼む文の「### <ID>（」の行から要望の ID を拾う。
fn ids_in(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter_map(|l| l.strip_prefix("### "))
        .filter_map(|l| l.split('（').next())
        .map(str::to_owned)
        .collect()
}

impl mm_improve::Agent for FakeAgent {
    fn run(&self, req: &AgentRequest<'_>) -> AgentEnd {
        self.calls
            .lock()
            .unwrap()
            .push((req.resume, req.prompt.to_owned()));
        self.budgets.lock().unwrap().push(req.settings.budget_usd);
        let first = self.calls.lock().unwrap()[0].1.clone();
        let next = self.script.lock().unwrap().pop_front();
        match next {
            Some(Script::Write(files)) => {
                for (path, body) in files {
                    let p = req.cwd.join(path);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, body).unwrap();
                }
                done_for(&first)
            }
            Some(Script::Say(m)) => AgentEnd::Finished {
                last_message: m.to_owned(),
            },
            Some(Script::Limit(t)) => AgentEnd::UsageLimit { resets_at: t },
            Some(Script::Run(f)) => f(req, &first),
            None => AgentEnd::Failed {
                reason: "台本が尽きた".into(),
            },
        }
    }
}

/// 決まった判定を返す検査の偽物。
#[derive(Default)]
pub struct FakeChecks {
    pub outcomes: Vec<CheckOutcome>,
    /// verify に渡された変えたファイル。
    pub seen: Mutex<Vec<String>>,
    /// verify の間に置く、すぐ止める印のファイル（検査の最中に人が止めたことにする）。
    pub stop_mark: Option<PathBuf>,
}

impl FakeChecks {
    pub fn passing() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn with(outcomes: Vec<CheckOutcome>) -> Arc<Self> {
        Arc::new(Self {
            outcomes,
            ..Default::default()
        })
    }

    /// 検査の最中に止められる。
    pub fn stopped_during_verify(data: &Path) -> Arc<Self> {
        Arc::new(Self {
            stop_mark: Some(data.join("stop")),
            ..Default::default()
        })
    }
}

impl Checks for FakeChecks {
    fn baseline(&self, _cx: &CheckCx<'_>) -> Result<Baseline> {
        Ok(Baseline::default())
    }

    fn verify(&self, cx: &CheckCx<'_>, _baseline: &Baseline) -> Vec<CheckOutcome> {
        self.seen.lock().unwrap().extend(cx.changed.iter().cloned());
        if let Some(mark) = &self.stop_mark {
            std::fs::write(mark, "stop").unwrap();
        }
        self.outcomes.clone()
    }
}

/// PR を記録するだけの偽物。
#[derive(Default)]
pub struct FakeForge {
    /// （ブランチ、題、本文）。
    pub prs: Mutex<Vec<(String, String, String)>>,
}

impl Forge for FakeForge {
    fn open_pr(&self, pr: &PullRequest<'_>) -> Result<String> {
        let mut prs = self.prs.lock().unwrap();
        prs.push((
            pr.branch.to_owned(),
            pr.title.to_owned(),
            pr.body.to_owned(),
        ));
        Ok(format!("https://example.invalid/pr/{}", prs.len()))
    }
}
