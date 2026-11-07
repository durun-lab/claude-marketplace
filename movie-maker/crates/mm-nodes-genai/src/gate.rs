//! 生成の前の関門。見積もり（[`Quote`]）、人の承認（[`Approval`]）、自動の改善の回の中での拒否。

use std::path::PathBuf;

use serde::Serialize;

use mm_ops::Entry;
use mm_types::{LayerId, MmError};

use crate::provider::Estimate;
use crate::request::Media;

/// 自動の改善の回の印。`mm-improve` が回の `claude -p` に回の ID を入れて渡し、子のプロセス（`mm` など）が受け継ぐ。
/// 名前は `mm_improve::ROUND_ENV` と同じ（試験で確かめる）。
pub const IMPROVE_ROUND_ENV: &str = "MM_IMPROVE_RUN";

/// 承認した上限を見積もりが超えたと見なす差。浮動小数の丸めで断らないための幅。
const COST_EPSILON_USD: f64 = 1e-9;

/// 生成の関門の失敗。
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    /// 許されない経路（自動の改善の回の中、MCP からの承認）。HTTP では 403 にする。
    #[error("生成 AI を呼べない: {0}")]
    Forbidden(String),
    /// 承認した見積もりと今の頼みが違う（承認の後にパラメータが変わった）。見積もりからやり直す。
    #[error("承認した見積もりと今の頼みが違う: {0}")]
    NotApproved(String),
    /// 見積もりが承認した上限を超えた。
    #[error("見積もり {estimated_usd} ドルが承認した上限 {approved_usd} ドルを超える")]
    OverBudget {
        approved_usd: f64,
        estimated_usd: f64,
    },
    /// 頼みの誤り、提供元の失敗、ファイルの読み書き。
    #[error(transparent)]
    Mm(#[from] MmError),
}

impl From<std::io::Error> for GenError {
    fn from(e: std::io::Error) -> Self {
        Self::Mm(MmError::Io(e))
    }
}

/// 今のプロセスが自動の改善の回の中なら、その回の ID。
/// 空の値も回の中と見なす。回の Claude が `MM_IMPROVE_RUN=` で上書きして印を外せないようにするため。
pub fn current_round() -> Option<String> {
    std::env::var_os(IMPROVE_ROUND_ENV).map(|v| v.to_string_lossy().into_owned())
}

/// 作ってある素材の使い回し。使い回すときは提供元を呼ばない。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reuse {
    /// 使い回せるものが無い。提供元を呼ぶ。
    None,
    /// 同じ鍵のファイルが `media/genai/` にある（取り消しの後など）。素材を足し直す。
    File { path: PathBuf },
    /// 同じ鍵の素材がプロジェクトにある。パラメータを向け直すだけ。
    Asset,
}

/// 見積もり。生成する前に必ず出し、人がこれを見て承認する。作るのは [`crate::GenAi::quote`] だけ。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub(crate) layer: LayerId,
    pub(crate) key: String,
    pub(crate) provider: String,
    pub(crate) media: Media,
    pub(crate) estimate: Estimate,
    pub(crate) reuse: Reuse,
}

impl Quote {
    /// 対象のレイヤー。
    pub fn layer(&self) -> &LayerId {
        &self.layer
    }

    /// 頼みの鍵（パラメータと提供元の版の BLAKE3）。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 提供元の名前。
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 生成するものの種類。
    pub fn media(&self) -> Media {
        self.media
    }

    /// 費用と時間。使い回すときは 0。
    pub fn estimate(&self) -> Estimate {
        self.estimate
    }

    /// 使い回し。
    pub fn reuse(&self) -> &Reuse {
        &self.reuse
    }
}

/// 人の承認の印。見積もりの鍵と費用の上限を持つ。作るのは [`crate::GenAi::approve`] だけで、
/// 自動の改善の回の中と MCP の経路からは作れない。
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub(crate) layer: LayerId,
    pub(crate) key: String,
    pub(crate) max_usd: f64,
    pub(crate) entry: Entry,
}

impl Approval {
    /// 承認した操作経路。
    pub fn entry(&self) -> Entry {
        self.entry
    }

    /// 承認した費用の上限（米ドル）。
    pub fn max_usd(&self) -> f64 {
        self.max_usd
    }

    /// 今の見積もりがこの承認の内か。
    pub(crate) fn covers(&self, quote: &Quote) -> Result<(), GenError> {
        if quote.layer != self.layer || quote.key != self.key {
            return Err(GenError::NotApproved(format!(
                "承認はレイヤー {} の鍵 {}、今はレイヤー {} の鍵 {}",
                self.layer, self.key, quote.layer, quote.key
            )));
        }
        if quote.estimate.cost_usd > self.max_usd + COST_EPSILON_USD {
            return Err(GenError::OverBudget {
                approved_usd: self.max_usd,
                estimated_usd: quote.estimate.cost_usd,
            });
        }
        Ok(())
    }
}

/// 承認してよい経路か。提案（RFC 0078）と同じく、Claude の経路（MCP と CLI）は自分の頼みを自分で承認できない。
pub(crate) fn check_approver(entry: Entry) -> Result<(), GenError> {
    if entry.is_claude() {
        return Err(GenError::Forbidden(
            "Claude の経路（MCP、CLI）からは承認できない。人がアプリで承認する".into(),
        ));
    }
    Ok(())
}

/// 自動の改善の回の中なら断る。
pub(crate) fn check_round(round: Option<&str>) -> Result<(), GenError> {
    match round {
        Some(id) => Err(GenError::Forbidden(format!(
            "自動の改善の回（{id}）の中では費用のかかる操作を呼べない"
        ))),
        None => Ok(()),
    }
}
