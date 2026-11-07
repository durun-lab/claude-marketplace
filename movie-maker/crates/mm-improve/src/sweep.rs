//! 落ちた回の後片付け。回を動かしていたプロセスが、worktree を作ってから状態を書くまでの間や、回を終えてから
//! worktree とブランチを消すまでの間に落ちると、状態から外れた worktree とブランチが残る。次にロックを取ったときに消す。
//! ロック（`run.lock`）は OS のファイルのロックなので、プロセスが落ちれば OS が外す。

use std::collections::HashSet;
use std::path::Path;

use mm_types::Result;

use crate::files::read_json;
use crate::git::git;
use crate::round::{Ctx, REPORT_FILE};
use crate::state::{ImproveState, RunReport};

/// 回のブランチの接頭辞（round.rs の `start` が付ける）。
pub(crate) const BRANCH_PREFIX: &str = "fix/movie-maker-improve-";

/// git の worktree の一覧の 1 つ。
struct Listed {
    path: String,
    branch: Option<String>,
}

/// 状態の回のもの以外で、この置き場所の回が残した worktree、フォルダー、ブランチを消す。
/// 取り込めなかった回と PR までで止めた回のブランチは、人が見るので残す。
pub(crate) fn sweep(ctx: &Ctx, st: &ImproveState) -> Result<()> {
    let repo = &ctx.cfg.repo;
    git(repo, &["worktree", "prune"])?;
    let current = st.run.as_ref();
    let current_wt = current.map(|r| norm(&r.worktree.to_string_lossy()));
    let prefix = format!("{}/", norm(&ctx.cfg.worktrees_dir.to_string_lossy()));
    let mut owned = HashSet::new();
    for wt in worktrees(repo)? {
        let p = norm(&wt.path);
        if !p.starts_with(&prefix) || Some(&p) == current_wt.as_ref() {
            continue;
        }
        git(repo, &["worktree", "remove", "--force", &wt.path])?;
        owned.extend(wt.branch);
    }
    remove_strays(&ctx.cfg.worktrees_dir, current_wt.as_deref())?;
    let current_branch = current.map(|r| r.branch.as_str());
    for branch in run_branches(repo)? {
        let id = &branch[BRANCH_PREFIX.len()..];
        let ours = owned.contains(&branch) || ctx.cfg.run_dir(id).exists();
        if Some(branch.as_str()) == current_branch || !ours || kept(ctx, id) {
            continue;
        }
        // ほかの worktree が使っているブランチは git が消さない。残しても次の回は起こせる。
        drop(git(repo, &["branch", "-D", &branch]));
    }
    Ok(())
}

/// 比べるためのパス。git は Windows でも `/` で出し、大文字と小文字を区別しない。
fn norm(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_owned()
    }
}

fn worktrees(repo: &Path) -> Result<Vec<Listed>> {
    let out = git(repo, &["worktree", "list", "--porcelain"])?;
    let mut list: Vec<Listed> = Vec::new();
    for line in out.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            list.push(Listed {
                path: path.to_owned(),
                branch: None,
            });
        } else if let (Some(b), Some(last)) =
            (line.strip_prefix("branch refs/heads/"), list.last_mut())
        {
            last.branch = Some(b.to_owned());
        }
    }
    Ok(list)
}

/// git が知らないフォルダー（消しかけの worktree）を消す。
fn remove_strays(dir: &Path, current: Option<&str>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        if path.is_dir() && Some(norm(&path.to_string_lossy()).as_str()) != current {
            std::fs::remove_dir_all(&path)?;
        }
    }
    Ok(())
}

fn run_branches(repo: &Path) -> Result<Vec<String>> {
    let pattern = format!("refs/heads/{BRANCH_PREFIX}*");
    let out = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", &pattern],
    )?;
    Ok(out
        .lines()
        .filter(|b| b.starts_with(BRANCH_PREFIX))
        .map(str::to_owned)
        .collect())
}

/// 記録が、ブランチを残す終わり方か。
fn kept(ctx: &Ctx, id: &str) -> bool {
    let report: Option<RunReport> = read_json(&ctx.cfg.run_dir(id).join(REPORT_FILE))
        .ok()
        .flatten();
    report.is_some_and(|r| r.outcome.keeps_branch())
}
