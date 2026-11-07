//! 提供元を差し替えられる層。画像・動画・音を作るサービス 1 つを [`Provider`] 1 つにする。

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use mm_types::{MmError, Rational, RationalTime, Result};

use crate::request::{GenRequest, Media};

/// 生成の前に出す見積もり。提供元が API を呼ばずに自分の料金表から出す。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    /// 費用（米ドル）。
    pub cost_usd: f64,
    /// かかる時間の目安（秒）。
    pub seconds: f64,
}

impl Estimate {
    /// 費用も時間もかからない（作ってある素材を使い回すとき）。
    pub const FREE: Self = Self {
        cost_usd: 0.0,
        seconds: 0.0,
    };
}

/// 提供元が返した生成物。
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// ファイルの中身。
    pub bytes: Vec<u8>,
    /// ファイルの拡張子（`png`、`wav` など）。デコーダーが形式を見分けるのに使う。
    pub extension: &'static str,
    /// 長さ。画像は 0。
    pub duration: RationalTime,
    /// 動画のフレームレート。画像と音は `None`。
    pub fps: Option<Rational>,
}

/// 生成 AI の提供元。実装はサービスごとに 1 つ。
///
/// 契約:
/// - `estimate` は外へ通信せず、費用のかかる呼び出しをしない（見積もりは何度でも呼ばれる）。
/// - `generate` だけが費用のかかる呼び出しをする。呼ぶのは承認を確かめた [`crate::GenAi::generate`] だけ。
/// - 同じ `revision` と頼みには同じ中身を返すよう努める（シードを渡す）。返せない提供元でも、キーが同じなら素材を使い回す。
pub trait Provider: Send + Sync {
    /// 名前。`gen.ai` の `provider` のパラメータの値。
    fn id(&self) -> &'static str;
    /// モデルの名前とバージョン。頼みのキーに混ぜる。
    fn revision(&self) -> &str;
    /// 作れる種類か。
    fn supports(&self, media: Media) -> bool;
    /// 見積もり。
    fn estimate(&self, req: &GenRequest) -> Result<Estimate>;
    /// 生成する。
    fn generate(&self, req: &GenRequest) -> Result<Generated>;
}

/// 提供元の表。名前で引く。
#[derive(Clone, Default)]
pub struct Providers {
    by_id: BTreeMap<&'static str, Arc<dyn Provider>>,
}

impl Providers {
    /// 空の表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 足す。同じ名前があれば `InvalidArgument`。
    pub fn with(mut self, provider: Arc<dyn Provider>) -> Result<Self> {
        let id = provider.id();
        if self.by_id.insert(id, provider).is_some() {
            return Err(MmError::InvalidArgument(format!("提供元 {id} が 2 つある")));
        }
        Ok(self)
    }

    /// 名前で引く。
    pub fn get(&self, id: &str) -> Result<&Arc<dyn Provider>> {
        self.by_id
            .get(id)
            .ok_or_else(|| MmError::NotFound(format!("生成 AI の提供元 {id}")))
    }

    /// 登録されている名前。
    pub fn ids(&self) -> Vec<&'static str> {
        self.by_id.keys().copied().collect()
    }
}
