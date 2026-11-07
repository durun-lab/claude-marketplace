//! PR までで止めるときの PR の作り方。[`Forge`] を差し替えられる層にし、実際の実装は [`GhForge`]（`git push` と `gh pr create`）。

use std::path::Path;
use std::process::Command;

use mm_types::{MmError, Result};

use crate::git::git;
use crate::proc::run;

/// 開く PR。
pub struct PullRequest<'a> {
    /// ルート。
    pub repo: &'a Path,
    /// 回のブランチ。
    pub branch: &'a str,
    /// 本線。
    pub base: &'a str,
    /// タイトル。
    pub title: &'a str,
    /// 本文。
    pub body: &'a str,
}

/// PR を作る相手。
pub trait Forge: Send + Sync {
    /// PR を作って URL を返す。
    fn open_pr(&self, pr: &PullRequest<'_>) -> Result<String>;
}

/// `git push -u origin <branch>` と `gh pr create`。マージはしない（人が承認する）。
#[derive(Clone, Copy, Debug, Default)]
pub struct GhForge;

impl Forge for GhForge {
    fn open_pr(&self, pr: &PullRequest<'_>) -> Result<String> {
        git(pr.repo, &["push", "-u", "origin", pr.branch])?;
        let ran = run(Command::new("gh").current_dir(pr.repo).args([
            "pr", "create", "--base", pr.base, "--head", pr.branch, "--title", pr.title, "--body",
            pr.body,
        ]))?;
        if !ran.ok {
            return Err(MmError::Internal(format!("gh pr create: {}", ran.tail(5))));
        }
        let url = ran
            .stdout
            .lines()
            .rev()
            .find(|l| l.starts_with("http"))
            .unwrap_or("");
        Ok(url.trim().to_owned())
    }
}
