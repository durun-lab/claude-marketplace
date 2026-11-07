//! 生成の頼み。`gen.ai` のパラメータから作り、同じ頼みを同じキー（BLAKE3）にする。

use serde::{Deserialize, Serialize};

use mm_types::{MmError, ParamValue, Params, RationalTime, Result};

/// 提供元の名前のパラメータ。
pub const PROVIDER_PARAM: &str = "provider";
/// 生成するものの種類のパラメータ（`image`、`video`、`audio`）。
pub const MEDIA_PARAM: &str = "media";
/// プロンプトのパラメータ。
pub const PROMPT_PARAM: &str = "prompt";
/// 幅（画素）のパラメータ。
pub const WIDTH_PARAM: &str = "width";
/// 高さ（画素）のパラメータ。
pub const HEIGHT_PARAM: &str = "height";
/// 長さ（秒）のパラメータ。画像では使わない。
pub const DURATION_PARAM: &str = "duration";
/// シードのパラメータ。
pub const SEED_PARAM: &str = "seed";
/// 生成した素材の参照のパラメータ。生成が書き、利用者は書かない。
pub const ASSET_PARAM: &str = "asset";

const MS_PER_SEC: f64 = 1000.0;
/// シードと大きさを整数として正確に持てる上限（f64 の仮数部）。
const MAX_EXACT_INT: f64 = 9_007_199_254_740_992.0;

/// 生成するものの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Image,
    Video,
    Audio,
}

impl Media {
    /// パラメータの選択肢の名前。
    pub const NAMES: [&'static str; 3] = ["image", "video", "audio"];

    /// パラメータの値から。
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            other => Err(MmError::InvalidArgument(format!(
                "gen.ai の media に {other} は無い"
            ))),
        }
    }

    /// 画を持つか。
    pub fn has_picture(self) -> bool {
        matches!(self, Self::Image | Self::Video)
    }

    /// 長さを持つか。
    pub fn has_duration(self) -> bool {
        matches!(self, Self::Video | Self::Audio)
    }
}

/// 生成の頼み。種類に関係しない値は 0 に揃え、変えてもキーが変わらないようにする。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenRequest {
    pub provider: String,
    pub media: Media,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub seed: u64,
}

impl GenRequest {
    /// 既定値で埋めたパラメータから。
    pub fn from_params(params: &Params) -> Result<Self> {
        let media = Media::parse(text(params, MEDIA_PARAM)?)?;
        let (width, height) = if media.has_picture() {
            (
                whole(params, WIDTH_PARAM, 1.0)?,
                whole(params, HEIGHT_PARAM, 1.0)?,
            )
        } else {
            (0, 0)
        };
        let duration_ms = if media.has_duration() {
            let secs = number(params, DURATION_PARAM)?;
            if !(secs > 0.0 && secs.is_finite()) {
                return Err(MmError::InvalidArgument(format!(
                    "gen.ai の duration は正の秒: {secs}"
                )));
            }
            (secs * MS_PER_SEC).round() as u64
        } else {
            0
        };
        Ok(Self {
            provider: text(params, PROVIDER_PARAM)?.to_owned(),
            media,
            prompt: text(params, PROMPT_PARAM)?.to_owned(),
            width: u32::try_from(width).map_err(|_| too_big(WIDTH_PARAM))?,
            height: u32::try_from(height).map_err(|_| too_big(HEIGHT_PARAM))?,
            duration_ms,
            seed: whole(params, SEED_PARAM, 0.0)?,
        })
    }

    /// 長さ。画像は 0。
    pub fn duration(&self) -> RationalTime {
        RationalTime::new(i128::from(self.duration_ms), MS_PER_SEC as u32)
            .unwrap_or(RationalTime::ZERO)
    }

    /// 頼みのキー。提供元のバージョン（モデルの名前）を混ぜ、モデルを変えたら別の素材にする。
    pub fn key(&self, revision: &str) -> String {
        let mut h = blake3::Hasher::new();
        h.update(revision.as_bytes());
        h.update(&[0]);
        // フィールドの順が決まった構造体の JSON なので、同じ頼みは同じバイト列になる。
        h.update(&serde_json::to_vec(self).unwrap_or_default());
        h.finalize().to_hex().to_string()
    }
}

fn get<'a>(params: &'a Params, name: &str) -> Result<&'a ParamValue> {
    params
        .get(name)
        .ok_or_else(|| MmError::InvalidArgument(format!("gen.ai の {name} が無い")))
}

fn text<'a>(params: &'a Params, name: &str) -> Result<&'a str> {
    get(params, name)?
        .as_text()
        .ok_or_else(|| MmError::InvalidArgument(format!("gen.ai の {name} は文字")))
}

fn number(params: &Params, name: &str) -> Result<f64> {
    get(params, name)?
        .as_number()
        .ok_or_else(|| MmError::InvalidArgument(format!("gen.ai の {name} は数")))
}

/// `min` 以上の整数として読む。
fn whole(params: &Params, name: &str, min: f64) -> Result<u64> {
    let v = number(params, name)?;
    if v.fract() != 0.0 || v < min || v > MAX_EXACT_INT {
        return Err(MmError::InvalidArgument(format!(
            "gen.ai の {name} は {min} 以上の整数: {v}"
        )));
    }
    Ok(v as u64)
}

fn too_big(name: &str) -> MmError {
    MmError::InvalidArgument(format!("gen.ai の {name} が大きすぎる"))
}
