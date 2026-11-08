//! `lottie` のノードの種類。プロジェクトの素材の Lottie（JSON）を、レイヤーの時刻で決めたフレームで CPU に描く。
//!
//! 読み込みは velato、描画は vello_cpu（README の「実装の選定」）。描いた画像は線形の premultiplied の f32 で返し、
//! レンダラーがテクスチャにする。アニメーションはレイヤーの画布（`cx.size`）に縦横比を保って収める。

mod cache;
mod raster;
mod timing;

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use velato::Composition;
use vello_cpu::kurbo::Affine;

use mm_node_api::{file_digest, Cost, FrameCx, FrameRef, NodeKind, Traits};
use mm_types::{
    AssetId, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, Size,
};

use crate::cache::{FrameJob, LottieCache};
use crate::timing::{Timing, DEFAULT_SPEED};

/// 種類の名前。
pub const KIND_ID: &str = "lottie";
/// 種類のバージョン。パラメータの形を変えたら上げる。
pub const KIND_VERSION: u32 = 1;
/// Lottie の JSON の素材（`AssetRef`）を持つパラメータの名前。
pub const ANIMATION_PARAM: &str = "animation";

/// 速さの上限。これより速いと 1 フレームの間にループを何周もし、見た目が乱れるだけになる。
const MAX_SPEED: f64 = 16.0;
/// `fit` の選択肢。`contain` は縦横比を保って画布に収める既定、`none` はアニメーションの画素のまま左上に置く。
const FIT_CONTAIN: &str = "contain";
const FIT_NONE: &str = "none";

/// Lottie のアニメーション。読んだアニメーションと描いたフレームを覚え、同じ入力では描き直さない。
#[derive(Default)]
pub struct LottieKind {
    cache: Mutex<LottieCache>,
}

impl std::fmt::Debug for LottieKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LottieKind")
    }
}

/// 素材。`AssetRef` が無いか空なら `None`。
fn animation_asset(params: &Params) -> Option<&AssetId> {
    params
        .get(ANIMATION_PARAM)
        .and_then(ParamValue::as_asset)
        .filter(|id| !id.as_str().is_empty())
}

/// 素材のファイル。素材をパスに直すのは、プロジェクトのデータを持つ呼び出し側（`InputFrames::asset_path`）。
fn animation_path(cx: &FrameCx<'_>, id: &AssetId) -> Result<PathBuf> {
    cx.inputs
        .asset_path(id)
        .ok_or_else(|| MmError::NotFound(format!("lottie の素材 {id}")))
}

/// アニメーションの座標から画像の画素への変形。アニメーションの大きさが 0 なら `None`。
fn placement(composition: &Composition, params: &Params, size: Size, scale: f32) -> Option<Affine> {
    let (aw, ah) = (composition.width as f64, composition.height as f64);
    if aw <= 0.0 || ah <= 0.0 {
        return None;
    }
    let fit = params.get("fit").and_then(ParamValue::as_text);
    if fit == Some(FIT_NONE) {
        return Some(Affine::scale(f64::from(scale)));
    }
    let (w, h) = (f64::from(size.width), f64::from(size.height));
    let s = (w / aw).min(h / ah);
    Some(Affine::translate(((w - aw * s) / 2.0, (h - ah * s) / 2.0)) * Affine::scale(s))
}

impl NodeKind for LottieKind {
    fn id(&self) -> &'static str {
        KIND_ID
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        ParamSchema::new(vec![
            ParamSpec::new(
                ANIMATION_PARAM,
                ParamType::AssetRef,
                ParamValue::AssetRef(AssetId::new("")),
            ),
            ParamSpec::new(
                "speed",
                ParamType::Number {
                    min: Some(0.0),
                    max: Some(MAX_SPEED),
                },
                ParamValue::Number(DEFAULT_SPEED),
            ),
            ParamSpec::new("loop", ParamType::Bool, ParamValue::Bool(true)),
            ParamSpec::new(
                "start_frame",
                ParamType::Number {
                    min: Some(0.0),
                    max: None,
                },
                ParamValue::Number(0.0),
            ),
            ParamSpec::new(
                "fit",
                ParamType::Choice {
                    options: vec![FIT_CONTAIN.into(), FIT_NONE.into()],
                },
                ParamValue::Choice(FIT_CONTAIN.into()),
            ),
        ])
    }

    fn traits(&self) -> Traits {
        Traits {
            vector: true,
            cost: Cost::Medium,
            ..Traits::PIXEL
        }
    }

    fn decodes_asset(&self) -> bool {
        false
    }

    /// 素材の JSON の中身のハッシュ。素材は取り込んだ後に書き換えられうる（`contentHash` は取り込み時の値）。
    /// 素材のファイルを読めなければ `None`（描けないのでキャッシュしない）。
    fn cache_fingerprint(
        &self,
        params: &Params,
        asset_path: &dyn Fn(&AssetId) -> Option<PathBuf>,
    ) -> Option<String> {
        match animation_asset(params) {
            Some(id) => file_digest(&asset_path(id)?),
            None => Some(String::new()),
        }
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if from != KIND_VERSION {
            return Err(MmError::Unsupported(format!(
                "{KIND_ID} のバージョン {from} からの移行が無い"
            )));
        }
        let params: Params = serde_json::from_value(old)?;
        self.params().validate(&params)?;
        Ok(params)
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let transparent = FrameRef::Transparent { size: cx.size };
        let Some(id) = animation_asset(cx.params) else {
            return Ok(transparent);
        };
        let path = animation_path(cx, id)?;
        // 描く途中の panic はレンダラーが受けるので、毒の入った覚えも中身を使い続ける（描く前に画布を空にする）。
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let composition = cache.composition(&path)?;
        let Some(transform) = placement(&composition, cx.params, cx.size, cx.scale) else {
            return Ok(transparent);
        };
        // 時刻は素材の時間軸（レイヤーの先頭が 0。トリムと速さの写像はレンダラーが済ませてある）。
        let frame = Timing::of(cx.params).frame_at(
            cx.source_time.to_f64(),
            &composition.frames,
            composition.frame_rate,
        );
        let job = FrameJob {
            path: &path,
            frame,
            size: cx.size,
            transform,
        };
        Ok(FrameRef::Cpu(cache.frame(&composition, &job)?))
    }
}
