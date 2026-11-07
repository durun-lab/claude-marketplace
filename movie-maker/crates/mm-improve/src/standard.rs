//! 実際の部品（`claude -p`、cargo の検査、`gh` の PR）での組み立て。アプリと `mm improve` が同じものを使う。

use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::ClaudeCli;
use crate::checks::CargoChecks;
use crate::config::ImproveConfig;
use crate::forge::GhForge;
use crate::improver::Improver;

/// 回の `CARGO_TARGET_DIR`。呼んだ人の指定があればそれ、なければ置き場所の `target/`。
pub fn run_target_dir(cfg: &ImproveConfig) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| cfg.data_dir.join("target"))
}

impl Improver {
    /// 実際の部品で組む。`claude -p` と検査は同じ `CARGO_TARGET_DIR` を使い、incremental を切る
    /// （incremental の target は 1 つで十数 GB になる）。`tune` で検査の既定（性能のシナリオなど）を変える。
    pub fn standard(cfg: ImproveConfig, tune: impl FnOnce(&mut CargoChecks)) -> Self {
        let target = run_target_dir(&cfg);
        let mut checks = CargoChecks::new(&target);
        tune(&mut checks);
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
        Self::new(cfg, Arc::new(agent), Arc::new(checks), Arc::new(GhForge))
    }
}
