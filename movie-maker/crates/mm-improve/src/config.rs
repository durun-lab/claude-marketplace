//! 置き場所（[`ImproveConfig`]）と、人が変える設定（[`Settings`]）。

use std::path::{Path, PathBuf};

use mm_types::{MmError, Result};
use serde::{Deserialize, Serialize};

use crate::git;

/// 回の予算の既定（ドル）。`claude -p --max-budget-usd` に渡す。
pub const DEFAULT_BUDGET_USD: f64 = 5.0;
/// 回の時間の既定（分）。超えたら `claude -p` を止める。
pub const DEFAULT_WALL_MINUTES: u32 = 60;
/// `mm perf compare` の閾値の既定（5%）。
pub const DEFAULT_PERF_THRESHOLD: f64 = 0.05;

/// 置き場所。アプリと CLI が同じ `data_dir` を指せば、同じ要望と回の状態を共有する。
#[derive(Clone, Debug)]
pub struct ImproveConfig {
    /// 要望、回の状態、設定の置き場所。
    pub data_dir: PathBuf,
    /// git の作業ツリーの根（`git rev-parse --show-toplevel`）。
    pub repo: PathBuf,
    /// 根から movie-maker までの相対パス（`movie-maker`）。根そのものなら空。
    pub subdir: PathBuf,
    /// 取り込む先のブランチ。人の main のチェックアウトには並行の WIP があることが多いので、既定は専用の
    /// `local/movie-maker`（durun-dashboard の `local/dashboard` と同じ型）。
    pub mainline: String,
    /// 取り込む先のブランチが無いときに切る元で、PR までで止めるときの PR の向き先。
    pub upstream: String,
    /// 回の worktree の置き場所。
    pub worktrees_dir: PathBuf,
}

/// 取り込む先のブランチの既定。
pub const DEFAULT_MAINLINE: &str = "local/movie-maker";
/// 切る元と PR の向き先の既定。
pub const DEFAULT_UPSTREAM: &str = "main";

impl ImproveConfig {
    /// `data_dir` と根を決めて作る。worktree は `data_dir/worktrees/` に置く。
    pub fn new(data_dir: impl Into<PathBuf>, repo: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            worktrees_dir: data_dir.join("worktrees"),
            data_dir,
            repo: repo.into(),
            subdir: PathBuf::new(),
            mainline: DEFAULT_MAINLINE.into(),
            upstream: DEFAULT_UPSTREAM.into(),
        }
    }

    /// 根からの相対パスを決める。
    pub fn with_subdir(mut self, subdir: impl Into<PathBuf>) -> Self {
        self.subdir = subdir.into();
        self
    }

    /// 取り込む先のブランチを決める。
    pub fn with_mainline(mut self, mainline: impl Into<String>) -> Self {
        self.mainline = mainline.into();
        self
    }

    /// 切る元と PR の向き先を決める。
    pub fn with_upstream(mut self, upstream: impl Into<String>) -> Self {
        self.upstream = upstream.into();
        self
    }

    /// `start` を含む git の作業ツリーから、根と相対パスを引いて作る。
    pub fn discover(data_dir: impl Into<PathBuf>, start: &Path) -> Result<Self> {
        let top = git::git(start, &["rev-parse", "--show-toplevel"])?;
        let prefix = git::git(start, &["rev-parse", "--show-prefix"])?;
        Ok(Self::new(data_dir, PathBuf::from(top)).with_subdir(prefix.trim_end_matches('/')))
    }

    /// 既定の置き場所。`MM_IMPROVE_DIR`、なければ `%LOCALAPPDATA%/movie-maker/improve`、
    /// それもなければ `$HOME/.local/share/movie-maker/improve`。
    pub fn default_data_dir() -> Result<PathBuf> {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        if let Some(d) = var("MM_IMPROVE_DIR") {
            return Ok(PathBuf::from(d));
        }
        if let Some(d) = var("LOCALAPPDATA") {
            return Ok(PathBuf::from(d).join("movie-maker").join("improve"));
        }
        var("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/movie-maker/improve"))
            .ok_or_else(|| MmError::NotFound("自動の改善の置き場所（MM_IMPROVE_DIR）".into()))
    }

    pub(crate) fn feedback_dir(&self) -> PathBuf {
        self.data_dir.join("feedback")
    }

    pub(crate) fn run_dir(&self, run_id: &str) -> PathBuf {
        self.data_dir.join("runs").join(run_id)
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }
}

/// 人が変える設定。`settings.json` に置く。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// ON のときだけ回を起こす。既定は OFF。
    pub enabled: bool,
    /// `claude -p --model`。
    pub model: Option<String>,
    /// `claude -p --effort`。
    pub effort: Option<String>,
    /// 回の予算（ドル）。
    pub budget_usd: Option<f64>,
    /// 回の時間の上限（分）。
    pub wall_minutes: u32,
    /// `mm perf compare` の閾値（割合）。
    pub perf_threshold: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: None,
            effort: None,
            budget_usd: Some(DEFAULT_BUDGET_USD),
            wall_minutes: DEFAULT_WALL_MINUTES,
            perf_threshold: DEFAULT_PERF_THRESHOLD,
        }
    }
}

/// [`Settings`] の一部を変える。`None` の項目は変えない。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    /// ON / OFF。
    pub enabled: Option<bool>,
    /// モデル。空の文字列で既定に戻す。
    pub model: Option<String>,
    /// effort。空の文字列で既定に戻す。
    pub effort: Option<String>,
    /// 予算（ドル）。0 以下で上限なし。
    pub budget_usd: Option<f64>,
    /// 時間の上限（分）。
    pub wall_minutes: Option<u32>,
    /// 閾値（割合）。
    pub perf_threshold: Option<f64>,
}

impl Settings {
    /// `patch` を当てる。
    pub fn apply(&mut self, patch: &SettingsPatch) {
        let text = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());
        if let Some(e) = patch.enabled {
            self.enabled = e;
        }
        if patch.model.is_some() {
            self.model = text(&patch.model);
        }
        if patch.effort.is_some() {
            self.effort = text(&patch.effort);
        }
        if let Some(b) = patch.budget_usd {
            self.budget_usd = (b > 0.0).then_some(b);
        }
        if let Some(w) = patch.wall_minutes {
            self.wall_minutes = w.max(1);
        }
        if let Some(t) = patch.perf_threshold {
            self.perf_threshold = t.max(0.0);
        }
    }
}
