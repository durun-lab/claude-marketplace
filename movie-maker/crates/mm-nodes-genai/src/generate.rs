//! 生成の流れ。見積もり → 人の承認 → 提供元の呼び出し → `media/genai/` への保存 → 素材を置くトランザクション。
//!
//! 生成は副作用（費用、ファイル）なので操作（`mm-ops` の `Op`）にしない。結果は普通の操作
//! （`addAsset` と `setParam`）のトランザクションで返し、呼ぶ側が適用する。取り消しとやり直しはそのまま効き、
//! 取り消してもファイルと付属の JSON は残るので、同じ頼みはもう一度提供元を呼ばずに置き直せる。

use std::path::{Path, PathBuf};

use mm_doc::{Asset, Layer, Project};
use mm_node_api::NodeKind;
use mm_ops::{AddAsset, Entry, Op, ParamRef, SetParam, Transaction};
use mm_types::{AssetId, ColorSpace, Id, LayerId, MmError, ParamValue, Params, RationalTime};

use crate::gate::{check_approver, check_round, current_round, Approval, GenError, Quote, Reuse};
use crate::kind::{GenAiKind, GENAI_KIND};
use crate::provider::{Estimate, Generated, Providers};
use crate::request::{GenRequest, Media, ASSET_PARAM};

/// 素材の ID に使う鍵の長さ（16 進の文字数）。
const ASSET_KEY_CHARS: usize = 16;
/// 素材を置くトランザクションの名前。
pub const GENERATE_TXN_NAME: &str = "生成 AI の素材を置く";

/// プロジェクトのフォルダーの中の、生成した素材の置き場所（`<project>/media/genai`）。
pub fn media_dir(project_root: &Path) -> PathBuf {
    project_root.join("media").join("genai")
}

/// 頼みの鍵から決まる素材の ID。同じ頼みは同じ素材になる。
pub fn asset_id_for(key: &str) -> AssetId {
    Id::new(format!("gen_{}", &key[..ASSET_KEY_CHARS.min(key.len())]))
}

/// 生成の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Generation {
    /// 素材を置くトランザクション。もう置いてあれば `None`。
    pub transaction: Option<Transaction>,
    /// 置く素材。
    pub asset: AssetId,
    /// 提供元を呼んだか（使い回したら偽）。
    pub called_provider: bool,
}

/// 生成 AI の関門と流れ。操作経路（アプリ、CLI）が 1 つ持つ。
pub struct GenAi {
    media_dir: PathBuf,
    providers: Providers,
    round: Option<String>,
}

impl GenAi {
    /// `media_dir` は [`media_dir`] の場所。自動の改善の回の印（[`crate::IMPROVE_ROUND_ENV`]）をここで読む。
    pub fn new(media_dir: impl Into<PathBuf>, providers: Providers) -> Self {
        Self {
            media_dir: media_dir.into(),
            providers,
            round: current_round(),
        }
    }

    /// 自動の改善の回の中として扱う。印は足せるだけで外せない。
    pub fn in_round(mut self, id: impl Into<String>) -> Self {
        self.round = Some(id.into());
        self
    }

    fn round(&self) -> Option<String> {
        self.round.clone().or_else(current_round)
    }

    /// レイヤーの `gen.ai` の今のパラメータでの見積もり。提供元の料金表から出し、費用はかからない。
    pub fn quote(&self, project: &Project, layer: &LayerId) -> Result<Quote, GenError> {
        let req = request_of(genai_layer(project, layer)?)?;
        let provider = self.providers.get(&req.provider)?;
        if !provider.supports(req.media) {
            return Err(MmError::Unsupported(format!(
                "提供元 {} は {:?} を作れない",
                req.provider, req.media
            ))
            .into());
        }
        let key = req.key(provider.revision());
        let reuse = self.find_reuse(project, &key);
        let estimate = match reuse {
            Reuse::None => provider.estimate(&req)?,
            _ => Estimate::FREE,
        };
        Ok(Quote {
            layer: layer.clone(),
            key,
            provider: req.provider,
            media: req.media,
            estimate,
            reuse,
        })
    }

    /// 見積もりを人が承認する。上限は見積もりの費用。MCP の経路と自動の改善の回の中では断る。
    pub fn approve(&self, quote: &Quote, entry: Entry) -> Result<Approval, GenError> {
        check_round(self.round().as_deref())?;
        check_approver(entry)?;
        Ok(Approval {
            layer: quote.layer.clone(),
            key: quote.key.clone(),
            max_usd: quote.estimate.cost_usd,
            entry,
        })
    }

    /// 承認の内で生成し、素材を置くトランザクションを返す。見積もりを出し直し、承認の後にパラメータが変わっていたら断る。
    pub fn generate(&self, project: &Project, approval: &Approval) -> Result<Generation, GenError> {
        check_round(self.round().as_deref())?;
        check_approver(approval.entry)?;
        let quote = self.quote(project, &approval.layer)?;
        approval.covers(&quote)?;
        let id = asset_id_for(&quote.key);
        let (asset, called_provider) = match &quote.reuse {
            Reuse::Asset => (None, false),
            Reuse::File { path } => (Some(read_asset(path)?), false),
            Reuse::None => {
                let req = request_of(genai_layer(project, &approval.layer)?)?;
                let made = self.providers.get(&req.provider)?.generate(&req)?;
                (Some(self.store(&quote.key, &id, req.media, made)?), true)
            }
        };
        // ファイルが消えた素材を作り直したときは、素材はもう在るので足さずに向け直す。
        let asset = asset.filter(|_| project.asset(&id).is_err());
        let layer = genai_layer(project, &approval.layer)?;
        Ok(Generation {
            transaction: place_ops(layer, &id, asset)
                .map(|ops| Transaction::new(GENERATE_TXN_NAME, ops)),
            asset: id,
            called_provider,
        })
    }

    /// 使い回せる素材かファイル。
    fn find_reuse(&self, project: &Project, key: &str) -> Reuse {
        let in_project = project
            .asset(&asset_id_for(key))
            .is_ok_and(|a| Path::new(&a.path).exists());
        if in_project {
            return Reuse::Asset;
        }
        let path = self.sidecar(key);
        if path.exists() {
            return Reuse::File { path };
        }
        Reuse::None
    }

    /// 素材の中身の隣に置く、素材の JSON。取り消しで素材がプロジェクトから外れても、これから足し直せる。
    fn sidecar(&self, key: &str) -> PathBuf {
        self.media_dir.join(format!("{key}.json"))
    }

    /// 生成物を `media/genai/<鍵>.<拡張子>` に書き、素材の JSON を隣に書く。
    fn store(
        &self,
        key: &str,
        id: &AssetId,
        media: Media,
        made: Generated,
    ) -> Result<Asset, GenError> {
        std::fs::create_dir_all(&self.media_dir)?;
        let path = self.media_dir.join(format!("{key}.{}", made.extension));
        write_whole(&path, &made.bytes)?;
        let asset = Asset {
            id: id.clone(),
            path: std::path::absolute(&path)?.to_string_lossy().into_owned(),
            content_hash: blake3::hash(&made.bytes).to_hex().to_string(),
            color_space: if media == Media::Image {
                ColorSpace::srgb()
            } else {
                ColorSpace::bt709()
            },
            duration: made.duration,
            fps: made.fps,
            timebase_offset: RationalTime::ZERO,
        };
        write_whole(
            &self.sidecar(key),
            &serde_json::to_vec_pretty(&asset).map_err(MmError::from)?,
        )?;
        Ok(asset)
    }
}

/// レイヤーを引き、`gen.ai` のレイヤーか確かめる。
fn genai_layer<'p>(project: &'p Project, id: &LayerId) -> Result<&'p Layer, GenError> {
    let (_, layer) = project.layer(id)?;
    if layer.source.kind != GENAI_KIND {
        return Err(MmError::InvalidArgument(format!(
            "レイヤー {id} は gen.ai でなく {}",
            layer.source.kind
        ))
        .into());
    }
    Ok(layer)
}

/// レイヤーの `gen.ai` の固定の値を既定値で埋めて頼みにする。キーフレームは見ない（生成は 1 回なので）。
fn request_of(layer: &Layer) -> Result<GenRequest, GenError> {
    let mut params = Params::new();
    for (name, p) in &layer.source.params {
        params.set(name.clone(), p.value.clone());
    }
    let params = params.with_defaults(&GenAiKind.params());
    Ok(GenRequest::from_params(&params)?)
}

/// 素材を足し（`asset` が `Some` のとき）、レイヤーの `asset` を向ける操作。もう向いていれば `None`。
fn place_ops(layer: &Layer, id: &AssetId, asset: Option<Asset>) -> Option<Vec<Op>> {
    let current = layer
        .source
        .params
        .get(ASSET_PARAM)
        .and_then(|p| p.value.as_asset());
    if asset.is_none() && current == Some(id) {
        return None;
    }
    let mut ops: Vec<Op> = Vec::new();
    if let Some(asset) = asset {
        ops.push(AddAsset { asset, index: None }.into());
    }
    ops.push(
        SetParam {
            target: ParamRef::layer(&layer.id, format!("source.{ASSET_PARAM}")),
            value: ParamValue::AssetRef(id.clone()),
            keyframes: None,
        }
        .into(),
    );
    Some(ops)
}

fn read_asset(path: &Path) -> Result<Asset, GenError> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes).map_err(MmError::from)?)
}

/// 書きかけを `.part` に書いてから置く。途中で止まったファイルを完成品と取り違えない。
fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), GenError> {
    let mut part = path.as_os_str().to_os_string();
    part.push(".part");
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, path)?;
    Ok(())
}
