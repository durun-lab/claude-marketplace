//! `gen.ai` のノードの種類。描くのは生成した素材だけで、毎フレーム生成しない。
//!
//! 素材は `asset` のパラメータ（`AssetRef`）なので、レンダラーは撮影素材と同じくデコーダーを開いて `cx.footage` に渡す。
//! 画像・動画・音の違いはデコーダーが吸収し、この種類は `footage` と同じ形で返す。生成する前（`asset` が空）は透明で無音。

use mm_audio_api::AudioBlock;
use mm_node_api::{audio_frames_in, AudioCx, FrameCx, FrameRef, NodeKind, Traits};
use mm_types::{
    Id, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, RationalTime, Rect, Result,
};

use crate::fake::FAKE_PROVIDER;
use crate::fit::fit_contain;
use crate::request::{
    GenRequest, Media, ASSET_PARAM, DURATION_PARAM, HEIGHT_PARAM, MEDIA_PARAM, PROMPT_PARAM,
    PROVIDER_PARAM, SEED_PARAM, WIDTH_PARAM,
};

/// 種類の名前。
pub const GENAI_KIND: &str = "gen.ai";
/// 種類のバージョン。
pub const KIND_VERSION: u32 = 1;
const DEFAULT_WIDTH: f64 = 1920.0;
const DEFAULT_HEIGHT: f64 = 1080.0;
const DEFAULT_SECONDS: f64 = 5.0;

/// 生成 AI の種類。
#[derive(Debug, Default, Clone, Copy)]
pub struct GenAiKind;

fn number(name: &str, min: f64, default: f64) -> ParamSpec {
    ParamSpec::new(
        name,
        ParamType::Number {
            min: Some(min),
            max: None,
        },
        ParamValue::Number(default),
    )
}

/// 素材を置く矩形（画布の画素）。縦横比を保って画布に収め、中央に置く。素材が無ければ画布の全体、音だけなら空。
fn picture_region(cx: &FrameCx<'_>) -> Rect {
    let canvas = cx.size.to_rect();
    let Some(media) = cx.footage.as_deref().map(|f| f.info().size) else {
        return canvas;
    };
    if media.width == 0 || media.height == 0 {
        return Rect::EMPTY;
    }
    let ratio = (canvas.width / media.width as f32).min(canvas.height / media.height as f32);
    let (w, h) = (
        (media.width as f32 * ratio).round().max(1.0),
        (media.height as f32 * ratio).round().max(1.0),
    );
    Rect::new(
        ((canvas.width - w) / 2.0).floor(),
        ((canvas.height - h) / 2.0).floor(),
        w,
        h,
    )
}

impl NodeKind for GenAiKind {
    fn id(&self) -> &'static str {
        GENAI_KIND
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        ParamSchema::new(vec![
            ParamSpec::new(
                PROVIDER_PARAM,
                ParamType::Text,
                ParamValue::Text(FAKE_PROVIDER.into()),
            ),
            ParamSpec::new(
                MEDIA_PARAM,
                ParamType::Choice {
                    options: Media::NAMES.map(String::from).to_vec(),
                },
                ParamValue::Choice(Media::NAMES[0].into()),
            ),
            ParamSpec::new(
                PROMPT_PARAM,
                ParamType::Text,
                ParamValue::Text(String::new()),
            ),
            number(WIDTH_PARAM, 1.0, DEFAULT_WIDTH),
            number(HEIGHT_PARAM, 1.0, DEFAULT_HEIGHT),
            number(DURATION_PARAM, 0.0, DEFAULT_SECONDS),
            number(SEED_PARAM, 0.0, 0.0),
            ParamSpec::new(
                ASSET_PARAM,
                ParamType::AssetRef,
                ParamValue::AssetRef(Id::new("")),
            ),
        ])
    }

    /// 生成は待つのが前提なので `offline`。描くのは素材のデコードだけなので再生には間に合う。
    fn traits(&self) -> Traits {
        Traits {
            offline: true,
            audio: true,
            ..Traits::PIXEL
        }
    }

    fn duration(&self, params: &Params) -> Option<RationalTime> {
        let req = GenRequest::from_params(&params.with_defaults(&self.params())).ok()?;
        req.media.has_duration().then(|| req.duration())
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if from != KIND_VERSION {
            return Err(MmError::Unsupported(format!(
                "{GENAI_KIND} のバージョン {from} からの移行が無い"
            )));
        }
        let params: Params = serde_json::from_value(old)?;
        self.params().validate(&params)?;
        Ok(params)
    }

    fn region_of_definition(&self, cx: &FrameCx<'_>) -> Rect {
        picture_region(cx)
    }

    /// 素材の `source_time` のフレーム。CPU の画像は画布に収めて返し、GPU のフレームは素材の大きさのまま借用して
    /// レンダラーが `region_of_definition` の矩形へ 1 回だけ縮める（`footage` と同じ）。
    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let (size, t) = (cx.size, cx.source_time);
        let Some(footage) = cx.footage.as_deref_mut() else {
            return Ok(FrameRef::Transparent { size });
        };
        if footage.info().size.width == 0 {
            return Ok(FrameRef::Transparent { size });
        }
        let frame = footage.frame_at(t)?;
        match (frame.image, frame.gpu) {
            (Some(image), _) => Ok(FrameRef::cpu(fit_contain(image, size))),
            (None, Some(handle)) => cx.gpu.import(&handle),
            (None, None) => Err(MmError::Corrupt(
                "デコーダーが画像も GPU のフレームも返さない".into(),
            )),
        }
    }

    fn render_audio(&self, cx: &mut AudioCx<'_>) -> Result<AudioBlock> {
        let has_audio = cx
            .footage
            .as_deref()
            .is_some_and(|f| f.info().audio.is_some());
        match cx.footage.as_deref_mut() {
            Some(footage) if has_audio => footage.audio_in(cx.source_range),
            _ => Ok(AudioBlock::silence(
                cx.sample_rate,
                cx.channels,
                audio_frames_in(cx.range, cx.sample_rate),
            )),
        }
    }
}
