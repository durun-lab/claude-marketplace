//! git の操作。回の worktree を作って消し、1 コミットにまとめ、本線へ取り込む。

use std::path::{Path, PathBuf};
use std::process::Command;

use mm_types::{MmError, Result};

/// `git -C dir <args>` を走らせ、標準出力を前後の空白を除いて返す。失敗したら標準エラーを添えて返す。
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
    Err(MmError::Internal(format!(
        "git {} が失敗した: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    )))
}

/// 終了コードだけを見る。
fn git_ok(dir: &Path, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()?
        .status
        .success())
}

fn path_str(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| MmError::InvalidArgument(format!("パスが UTF-8 でない: {}", p.display())))
}

/// `base` から `branch` を切って `path` に worktree を作る。
pub fn worktree_add(repo: &Path, path: &Path, branch: &str, base: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    git(
        repo,
        &["worktree", "add", "-b", branch, path_str(path)?, base],
    )
    .map(drop)
}

/// worktree を消す。`delete_branch` ならブランチも消す。消えていても失敗にしない。
pub fn worktree_remove(repo: &Path, path: &Path, branch: &str, delete_branch: bool) -> Result<()> {
    if path.exists() {
        git(repo, &["worktree", "remove", "--force", path_str(path)?])?;
    }
    git(repo, &["worktree", "prune"])?;
    if delete_branch && git_ok(repo, &["rev-parse", "--verify", "--quiet", branch])? {
        git(repo, &["branch", "-D", branch])?;
    }
    Ok(())
}

/// 回の変更を `base` からの 1 コミットにまとめる。回がコミットしていても、していなくても同じ形になる。
/// 変更が無ければ `None`。
pub fn squash(wt: &Path, base: &str, message: &str) -> Result<Option<String>> {
    git(wt, &["add", "-A"])?;
    git(wt, &["reset", "--soft", base])?;
    if git_ok(wt, &["diff", "--cached", "--quiet"])? {
        return Ok(None);
    }
    git(wt, &["commit", "-q", "-m", message])?;
    git(wt, &["rev-parse", "HEAD"]).map(Some)
}

/// 本線の先端に載せ直す。衝突したら取り消して `false`。
pub fn rebase(wt: &Path, onto: &str) -> Result<bool> {
    if git_ok(wt, &["rebase", "-q", onto])? {
        return Ok(true);
    }
    git_ok(wt, &["rebase", "--abort"])?;
    Ok(false)
}

/// `base` から HEAD までに変えたファイル（根からの相対パス）。
pub fn changed_files(wt: &Path, base: &str) -> Result<Vec<String>> {
    let out = git(wt, &["diff", "--name-only", base, "HEAD"])?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// `branch` をチェックアウトしている worktree。
fn checkout_of(repo: &Path, branch: &str) -> Result<Option<PathBuf>> {
    let list = git(repo, &["worktree", "list", "--porcelain"])?;
    let want = format!("branch refs/heads/{branch}");
    let mut current = None;
    for line in list.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            current = Some(PathBuf::from(p));
        } else if line == want {
            return Ok(current);
        }
    }
    Ok(None)
}

/// `branch` を本線へ早送りで取り込み、本線の新しい先端を返す。
/// 本線をチェックアウトしている作業ツリーがあれば、そこで `merge --ff-only` する（人の未コミットの変更は git が守る）。
/// なければブランチの参照だけを進める。
pub fn land(repo: &Path, mainline: &str, branch: &str) -> Result<String> {
    let new = git(repo, &["rev-parse", branch])?;
    let old = git(repo, &["rev-parse", mainline])?;
    if !git_ok(repo, &["merge-base", "--is-ancestor", &old, &new])? {
        return Err(MmError::Internal(format!(
            "{branch} は {mainline} の先端から続いていない"
        )));
    }
    match checkout_of(repo, mainline)? {
        Some(dir) => git(&dir, &["merge", "--ff-only", "-q", branch]).map(drop)?,
        None => git(
            repo,
            &["update-ref", &format!("refs/heads/{mainline}"), &new, &old],
        )
        .map(drop)?,
    }
    Ok(new)
}

/// `branch` が無ければ `from` から切る。チェックアウトはしない。
pub fn ensure_branch(repo: &Path, branch: &str, from: &str) -> Result<()> {
    let refname = format!("refs/heads/{branch}");
    if !git_ok(repo, &["rev-parse", "--verify", "--quiet", &refname])? {
        git(repo, &["branch", branch, from])?;
    }
    Ok(())
}

/// `rev` の `path` の中身。areas.toml を回の変更の前の版で読むのに使う。
pub fn show(repo: &Path, rev: &str, path: &str) -> Result<String> {
    git(repo, &["show", &format!("{rev}:{path}")])
}
