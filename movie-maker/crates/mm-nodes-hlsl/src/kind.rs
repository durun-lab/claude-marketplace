//! `shader.hlsl` の種類。パラメータと入力の形は `shader.wgsl` に揃え、入力を 2 つまで受ける。

use std::path::Path;
use std::sync::OnceLock;

use mm_gpu::WgpuGpu;
use mm_node_api::{
    Cost, FrameCx, FrameRef, GpuHelper, GpuTexture, InputSchema, InputSpec, NodeKind, Traits,
};
use mm_types::{
    AssetId, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, Size,
};

use crate::convert::convert;
use crate::dxc::Dxc;
use crate::pass::{Pass, PassCache};
use crate::source::MAX_INPUTS;

/// 種類のバージョン。パラメータの形を変えたら上げる。
pub const KIND_VERSION: u32 = 1;
/// HLSL の源の素材（`AssetRef`）を持つパラメータの名前。空でなければ `code` より優先する。
pub const ASSET_PARAM: &str = "asset";
/// HLSL の源を文字で持つパラメータの名前。
pub const CODE_PARAM: &str = "code";
/// `code` の源を誤りに出すときの名前。
pub const CODE_LABEL: &str = "code";

/// 数のパラメータの数。`p0`..`p7`。
const NUMBER_PARAMS: usize = 8;
/// uniform の f32 の数（64 バイト）。
const UNIFORM_FLOATS: usize = 16;

/// コードの既定。UV をそのまま色にする（`shader.wgsl` の既定と同じ絵）。
const DEFAULT_CODE: &str = "float4 mm_main(float2 uv) {\n    return float4(uv, 0.5, 1.0);\n}\n";

/// 利用者の HLSL を描く種類。DXC はプロセスで 1 回探し、無ければ描くたびに `Unsupported` を返す。
#[derive(Debug, Default)]
pub struct ShaderHlslKind {
    dxc: OnceLock<Option<Dxc>>,
    cache: PassCache,
}

impl ShaderHlslKind {
    /// 使う DXC を決めた種類。`None` は DXC の無い環境（試験で使う）。
    pub fn with_dxc(dxc: Option<Dxc>) -> Self {
        Self {
            dxc: OnceLock::from(dxc),
            cache: PassCache::default(),
        }
    }

    /// 使う DXC。まだ探していなければ探す。
    pub fn dxc(&self) -> Option<&Dxc> {
        self.dxc.get_or_init(Dxc::discover).as_ref()
    }
}

/// uniform（`MmU`）の中身。`size`、`time`、`scale`、`p0`..`p7` の順。
fn uniform_bytes(size: Size, time: f32, scale: f32, numbers: &[f32; NUMBER_PARAMS]) -> Vec<u8> {
    let mut v = [0.0_f32; UNIFORM_FLOATS];
    v[..4].copy_from_slice(&[size.width as f32, size.height as f32, time, scale]);
    v[4..4 + NUMBER_PARAMS].copy_from_slice(numbers);
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// 有限の数。NaN は GPU の上で全画素を NaN にするので 0 にする。
fn finite(params: &Params, name: &str) -> f32 {
    let v = params.number_or(name, 0.0) as f32;
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// 空でない素材の ID。
fn asset_of(params: &Params) -> Option<&AssetId> {
    params
        .get(ASSET_PARAM)
        .and_then(ParamValue::as_asset)
        .filter(|id| !id.as_str().is_empty())
}

/// HLSL の源と、誤りに出す名前。素材があればファイルを読み、名前はファイル名にする。
fn source_of(cx: &FrameCx<'_>) -> Result<(String, String)> {
    let Some(id) = asset_of(cx.params) else {
        let code = cx.params.get(CODE_PARAM).and_then(ParamValue::as_text);
        return Ok((code.unwrap_or(DEFAULT_CODE).to_string(), CODE_LABEL.into()));
    };
    let path = cx
        .inputs
        .asset_path(id)
        .ok_or_else(|| MmError::NotFound(format!("HLSL の素材 {id}")))?;
    let label = Path::new(&path)
        .file_name()
        .map_or_else(|| id.to_string(), |n| n.to_string_lossy().into_owned());
    Ok((std::fs::read_to_string(&path)?, label))
}

/// パスの入力にするテクスチャ。`made` が真なら、この呼び出しで作ったので使い終えたら解放する。
struct Input {
    id: u64,
    size: Size,
    made: bool,
}

fn input_texture(gpu: &mut WgpuGpu, frame: FrameRef) -> Result<Input> {
    let (frame, made) = match frame {
        FrameRef::Transparent { size } => (GpuHelper::allocate(gpu, size)?, true),
        FrameRef::Cpu(image) => (GpuHelper::upload(gpu, &image)?, true),
        gpu_frame @ FrameRef::Gpu(_) => (gpu_frame, false),
    };
    match frame {
        FrameRef::Gpu(tex) => Ok(Input {
            id: tex.id,
            size: tex.size,
            made,
        }),
        _ => Err(MmError::Internal("GPU のテクスチャにならなかった".into())),
    }
}

fn release(gpu: &mut WgpuGpu, inputs: &[Input]) {
    for tex in inputs.iter().filter(|t| t.made) {
        gpu.release_id(tex.id);
    }
}

/// フレームを全部テクスチャにする。途中で失敗したら作った分を解放する。
fn input_textures(gpu: &mut WgpuGpu, frames: Vec<FrameRef>) -> Result<Vec<Input>> {
    let mut out = Vec::with_capacity(frames.len());
    for frame in frames {
        match input_texture(gpu, frame) {
            Ok(tex) => out.push(tex),
            Err(e) => {
                release(gpu, &out);
                return Err(e);
            }
        }
    }
    if out.windows(2).any(|w| w[0].size != w[1].size) {
        release(gpu, &out);
        return Err(MmError::InvalidArgument(
            "shader.hlsl の入力の大きさが違う".into(),
        ));
    }
    Ok(out)
}

impl NodeKind for ShaderHlslKind {
    fn id(&self) -> &'static str {
        "shader.hlsl"
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        let mut params = vec![
            ParamSpec::new(
                CODE_PARAM,
                ParamType::Text,
                ParamValue::Text(DEFAULT_CODE.into()),
            ),
            ParamSpec::new(
                ASSET_PARAM,
                ParamType::AssetRef,
                ParamValue::AssetRef(AssetId::new("")),
            ),
        ];
        params.extend((0..NUMBER_PARAMS).map(|i| {
            ParamSpec::new(
                format!("p{i}"),
                ParamType::Number {
                    min: None,
                    max: None,
                },
                ParamValue::Number(0.0),
            )
        }));
        ParamSchema::new(params)
    }

    fn inputs(&self) -> InputSchema {
        let input = |name: &str| InputSpec {
            name: name.into(),
            optional: true,
        };
        InputSchema {
            inputs: vec![input("source"), input("other")],
        }
    }

    fn traits(&self) -> Traits {
        Traits {
            cost: Cost::Medium,
            ..Traits::PIXEL
        }
    }

    /// `asset` は HLSL の源のファイルで、デコーダーでは開けない。パスを引いて自分で読む。
    fn decodes_asset(&self) -> bool {
        false
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if from != KIND_VERSION {
            return Err(MmError::Unsupported(format!(
                "shader.hlsl のバージョン {from} からの移行が無い"
            )));
        }
        let params: Params = serde_json::from_value(old)?;
        self.params().validate(&params)?;
        Ok(params)
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let (code, label) = source_of(cx)?;
        let numbers: [f32; NUMBER_PARAMS] =
            std::array::from_fn(|i| finite(cx.params, &format!("p{i}")));
        let (time, scale, canvas, t) = (cx.time.to_f64() as f32, cx.scale, cx.size, cx.time);
        let frames = (0..cx.inputs.len().min(MAX_INPUTS))
            .map(|i| cx.inputs.frame(i, t))
            .collect::<Result<Vec<_>>>()?;
        let Some(gpu) = cx.gpu.as_any().downcast_mut::<WgpuGpu>() else {
            return Err(MmError::Unsupported(
                "shader.hlsl は wgpu の GPU が要る".into(),
            ));
        };
        let dxc = self.dxc().ok_or_else(|| {
            MmError::Unsupported(
                "shader.hlsl は SPIR-V を出せる DXC が要る（mm-nodes-hlsl の README）".into(),
            )
        })?;
        let wgsl = convert(dxc, &code, &label, frames.len())
            .map_err(|e| MmError::InvalidArgument(e.to_string()))?;
        let inputs = input_textures(gpu, frames)?;
        let size = inputs.first().map_or(canvas, |i| i.size);
        let ids: Vec<u64> = inputs.iter().map(|i| i.id).collect();
        let result = self.cache.run(
            gpu,
            &Pass {
                fragment: &wgsl,
                uniform: &uniform_bytes(size, time, scale, &numbers),
                inputs: &ids,
                size,
            },
        );
        release(gpu, &inputs);
        Ok(FrameRef::Gpu(GpuTexture { id: result?, size }))
    }
}
