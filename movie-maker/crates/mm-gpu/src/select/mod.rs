//! 使う GPU の指定（環境変数 `MM_GPU`）と、その解釈。
//!
//! 書き方は `CUDA_VISIBLE_DEVICES` と DXVK の `DXVK_FILTER_DEVICE_NAME` に倣う。指定があれば、
//! そのプロセスには合う GPU が 1 つだけ見え、ほかの GPU にはデバイスを作らない。合わなければ
//! 一覧を添えてエラーにし、別の GPU に黙って落ちない。

mod adapter;
#[cfg(windows)]
mod dx12;

use std::fmt;

use mm_types::{MmError, Result};

pub use adapter::{enumerate_adapters, find_adapter, find_adapter_id, select_adapter};
#[cfg(windows)]
pub(crate) use dx12::luid_value as dx12_luid;

/// 使う GPU を指定する環境変数。
pub const GPU_ENV: &str = "MM_GPU";

/// LUID の指定の接頭辞。
const LUID_PREFIX: &str = "luid:";
/// 番号の指定の接頭辞。
const INDEX_PREFIX: &str = "index:";
/// LUID を書く 16 進の桁数（64 ビット）。
const LUID_HEX_DIGITS: usize = 16;

/// 使う GPU の指定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuSelector {
    /// 名前の一部。大文字小文字を区別しない（`5090`、`radeon`）。
    Name(String),
    /// D3D12（DXGI）のアダプターの LUID（`luid:` に 16 進）。プロセスをまたいで同じ GPU を指す。
    Luid(u64),
    /// 一覧の番号（`index:` に 0 から）。一覧は合わないときのエラーに出る。
    Index(usize),
}

impl GpuSelector {
    /// 指定の文字列を読む。空白だけなら指定なしで `None`。
    pub fn parse(text: &str) -> Result<Option<Self>> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let lower = text.to_lowercase();
        if let Some(hex) = lower.strip_prefix(LUID_PREFIX) {
            let hex = hex.trim().trim_start_matches("0x");
            return u64::from_str_radix(hex, 16)
                .map(|luid| Some(Self::Luid(luid)))
                .map_err(|_| invalid(text, "luid: の後は 16 進の数"));
        }
        if let Some(n) = lower.strip_prefix(INDEX_PREFIX) {
            return n
                .trim()
                .parse()
                .map(|i| Some(Self::Index(i)))
                .map_err(|_| invalid(text, "index: の後は 0 からの番号"));
        }
        Ok(Some(Self::Name(text.to_string())))
    }

    /// 環境変数 `MM_GPU` を読む。無いか空なら `None`。
    pub fn from_env() -> Result<Option<Self>> {
        match std::env::var(GPU_ENV) {
            Ok(text) => Self::parse(&text),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(MmError::InvalidArgument(format!(
                "{GPU_ENV} が UTF-8 でない"
            ))),
        }
    }

    fn matches(&self, index: usize, candidate: &AdapterCandidate) -> bool {
        match self {
            Self::Name(part) => candidate.name.to_lowercase().contains(&part.to_lowercase()),
            Self::Luid(luid) => candidate.luid == Some(*luid),
            Self::Index(i) => *i == index,
        }
    }
}

impl fmt::Display for GpuSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(part) => write!(f, "{part}"),
            Self::Luid(luid) => write!(f, "{LUID_PREFIX}{luid:0LUID_HEX_DIGITS$x}"),
            Self::Index(i) => write!(f, "{INDEX_PREFIX}{i}"),
        }
    }
}

/// 選ぶ前の GPU。デバイスを作らずに分かることだけを持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCandidate {
    /// アダプターの名前。
    pub name: String,
    /// D3D12 の LUID。D3D12 でなければ `None`。
    pub luid: Option<u64>,
}

/// `candidates` のうち `selector` に合う最初の番号。合わなければ、指定の出どころ `label` と一覧を添えたエラー。
pub fn choose(
    label: &str,
    selector: &GpuSelector,
    candidates: &[AdapterCandidate],
) -> Result<usize> {
    candidates
        .iter()
        .enumerate()
        .position(|(i, c)| selector.matches(i, c))
        .ok_or_else(|| {
            MmError::InvalidArgument(format!(
                "{label}={selector} に合う GPU が無い。ある GPU: {}",
                describe(candidates)
            ))
        })
}

/// 一覧を「番号: 名前 (luid:…)」で並べる。指定に書ける形で出す。
fn describe(candidates: &[AdapterCandidate]) -> String {
    if candidates.is_empty() {
        return "（無し）".into();
    }
    candidates
        .iter()
        .enumerate()
        .map(|(i, c)| match c.luid {
            Some(luid) => format!("{INDEX_PREFIX}{i} {} ({})", c.name, GpuSelector::Luid(luid)),
            None => format!("{INDEX_PREFIX}{i} {}", c.name),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn invalid(text: &str, expected: &str) -> MmError {
    MmError::InvalidArgument(format!(
        "{GPU_ENV}={text} を読めない（{expected}）。書き方: 名前の一部、{LUID_PREFIX}<16 進>、{INDEX_PREFIX}<番号>"
    ))
}

#[cfg(test)]
mod tests;
