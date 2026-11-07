//! git の操作。回の worktree を作って消し、1 コミットにまとめ、本線へ取り込む。

use std::path::{Path, PathBuf};
use std::process::Command;

use mm_types::{MmError, Result};

/// git のファイルのモードのうち、中身の差分で判定できないもの（シンボリックリンクと submodule）。
const SPECIAL_MODES: [&str; 2] = ["120000", "160000"];

/// `git -C dir <args>` を実行し、標準出力を前後の空白を除いて返す。失敗したら標準エラーを添えて返す。
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    git_raw(dir, args).map(|s| s.trim().to_owned())
}

/// 標準出力をそのまま返す（`-z` の出力で、パスの前後の空白を削らない）。
fn git_raw(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(MmError::Internal(format!(
        "git {} が失敗した: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    )))
}

/// 変えた 1 つのファイル。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// ルートからの相対パス。
    pub path: String,
    /// 前か後がシンボリックリンクか submodule。
    pub special: bool,
}

/// `base` から `to`（`None` なら作業ツリー）までに変えたファイル（ルートからの相対）。
/// 名前の変更は、消したファイルと足したファイルの 2 つに分ける。変更先だけを見ると、領域の外のファイルを
/// 中へ移した変更が取り込めてしまうため。`-z` で読むので、日本語のパスも引用符に包まれない。
pub fn changes(dir: &Path, base: &str, to: Option<&str>) -> Result<Vec<Change>> {
    let mut args = vec!["diff", "--raw", "-z", "--no-renames", base];
    args.extend(to);
    Ok(parse_raw(&git_raw(dir, &args)?))
}

/// `git diff --raw -z` の「:<前のモード> <後のモード> <前> <後> <状態>\0<パス>\0」の並びを読む。
fn parse_raw(out: &str) -> Vec<Change> {
    let mut tokens = out.split('\0');
    let mut changes = Vec::new();
    while let (Some(meta), Some(path)) = (tokens.next(), tokens.next()) {
        let modes: Vec<&str> = meta.trim_start_matches(':').split(' ').take(2).collect();
        changes.push(Change {
            path: path.to_owned(),
            special: modes.iter().any(|m| SPECIAL_MODES.contains(m)),
        });
    }
    changes
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

/// `base` から HEAD までに変えたファイル（ルートからの相対パス。名前の変更は前と後の両方）。
pub fn changed_files(wt: &Path, base: &str) -> Result<Vec<String>> {
    Ok(changes(wt, base, Some("HEAD"))?
        .into_iter()
        .map(|c| c.path)
        .collect())
}

/// 作業ツリーの追跡していないファイル（ルートからの相対パス）。
pub fn untracked(dir: &Path) -> Result<Vec<String>> {
    let top = git(dir, &["rev-parse", "--show-toplevel"])?;
    let out = git_raw(
        Path::new(&top),
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
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

/// PR の土台。`origin/<upstream>` を取ってきて使い、遠隔が無いリポジトリ（テストのフィクスチャ）では手元の `<upstream>` を使う。
pub fn upstream_ref(repo: &Path, upstream: &str) -> Result<String> {
    // 取れなくても、前に取った `origin/<upstream>` か手元のブランチで続ける。
    git_ok(repo, &["fetch", "-q", "origin", upstream])?;
    let remote = format!("refs/remotes/origin/{upstream}");
    if git_ok(repo, &["rev-parse", "--verify", "--quiet", &remote])? {
        Ok(format!("origin/{upstream}"))
    } else {
        Ok(upstream.to_owned())
    }
}

/// worktree の HEAD のコミットだけを `onto` の先端に載せ替える（`branch` を `onto` から切り直して cherry-pick）。
/// 本線が `onto` より進んでいても、その先行分を PR に載せないため。衝突したら `branch` を元のコミットに戻して `false`。
pub fn replant(wt: &Path, branch: &str, onto: &str) -> Result<bool> {
    let commit = git(wt, &["rev-parse", "HEAD"])?;
    git(wt, &["checkout", "-q", "-B", branch, onto])?;
    if git_ok(wt, &["cherry-pick", &commit])? {
        return Ok(true);
    }
    git_ok(wt, &["cherry-pick", "--abort"])?;
    git(wt, &["checkout", "-q", "-B", branch, &commit])?;
    Ok(false)
}

/// `branch` が無ければ `from` から切る。チェックアウトはしない。
pub fn ensure_branch(repo: &Path, branch: &str, from: &str) -> Result<()> {
    let refname = format!("refs/heads/{branch}");
    if !git_ok(repo, &["rev-parse", "--verify", "--quiet", &refname])? {
        git(repo, &["branch", branch, from])?;
    }
    Ok(())
}

/// `rev` の `path` の中身。areas.toml を回の変更の前のコミットで読むのに使う。
pub fn show(repo: &Path, rev: &str, path: &str) -> Result<String> {
    git(repo, &["show", &format!("{rev}:{path}")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_diff_marks_symlinks_and_submodules() {
        let out = ":100644 100644 a b M\0src/a.txt\0:000000 120000 0 c A\0src/link\0:160000 000000 d 0 D\0sub\0";
        let got = parse_raw(out);
        let special: Vec<(&str, bool)> = got.iter().map(|c| (c.path.as_str(), c.special)).collect();
        assert_eq!(
            special,
            vec![("src/a.txt", false), ("src/link", true), ("sub", true)]
        );
    }
}
