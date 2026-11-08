//! 色の種類: `color.lut`（`.cube` の 3D LUT）と `color.convert`（`mm-color-api` の色空間の変換）。
//!
//! どちらも入力は作業色空間（線形の光）。`color.lut` は LUT の入力の色空間（`input_space`）に符号化してから引き、
//! 線形に戻して `intensity` で元と混ぜる。`color.convert` は入力の値を `from` の符号化値として読み、`to` の符号化値にする。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mm_color_api::{ColorTransform, TransformProgram};
use mm_node_api::{file_digest, FrameCx, FrameRef, InputSchema, InputSpec, NodeKind, Traits};
use mm_types::{
    transfer, AssetId, ColorSpace, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params,
    Result,
};

use crate::color::{apply, ColorPass};
use crate::gpu_pass::PassCache;
use crate::lut::{Lut, LUT_WGSL};
use crate::util::migrate_same_version;
use crate::KIND_VERSION;

/// LUT の入力の色空間の名前。`mm-color-api` の名前の表のうち、LUT でよく使う 3 つ。
const SPACES: [&str; 3] = ["lin_rec709", "srgb", "bt709"];
/// `color.lut` の画素の作り方のバージョン（定義域の外を端の値に足して持ち越す）。フレームのキャッシュの指紋に入れる。
const LUT_PIXEL_VERSION: &str = "carry.1";
/// LUT の `.cube` の素材（`AssetRef`）を持つパラメータの名前。
pub const LUT_PARAM: &str = "lut";

fn input_schema() -> InputSchema {
    InputSchema {
        inputs: vec![InputSpec {
            name: "source".into(),
            optional: false,
        }],
    }
}

fn source_frame(cx: &mut FrameCx<'_>) -> Result<FrameRef> {
    let t = cx.time;
    cx.inputs.frame(0, t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Linear,
    Srgb,
    Bt709,
}

impl Encoding {
    fn of(params: &Params) -> Self {
        match params.get("input_space").and_then(ParamValue::as_text) {
            Some("srgb") => Self::Srgb,
            Some("lin_rec709") => Self::Linear,
            _ => Self::Bt709,
        }
    }

    /// WGSL の `u.d.x` に入れる番号。
    fn code(self) -> f32 {
        match self {
            Self::Linear => 0.0,
            Self::Srgb => 1.0,
            Self::Bt709 => 2.0,
        }
    }

    fn encode(self, v: f32) -> f32 {
        match self {
            Self::Linear => v,
            Self::Srgb => transfer::linear_to_srgb(v),
            Self::Bt709 => transfer::linear_to_bt1886(v),
        }
    }

    fn decode(self, v: f32) -> f32 {
        match self {
            Self::Linear => v,
            Self::Srgb => transfer::srgb_to_linear(v),
            Self::Bt709 => transfer::bt1886_to_linear(v),
        }
    }
}

/// 読んだ LUT と、読んだ時のファイルの中身のハッシュ。
struct Loaded {
    path: PathBuf,
    digest: String,
    lut: Arc<Lut>,
}

/// `.cube` の 3D LUT を適用する。
#[derive(Default)]
pub struct ColorLutKind {
    cache: PassCache,
    loaded: Mutex<Option<Loaded>>,
}

impl std::fmt::Debug for ColorLutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ColorLutKind")
    }
}

impl ColorLutKind {
    /// `path` の LUT。中身のハッシュが同じなら読み直さない。種類の指紋と同じ値で覚え、フレームのキャッシュの
    /// キーと違う中身の LUT で描かない（更新時刻の刻みの中の書き換えも見分ける）。
    fn load(&self, path: &Path) -> Result<Arc<Lut>> {
        let path = path.to_path_buf();
        let digest = file_digest(&path)
            .ok_or_else(|| MmError::NotFound(format!("LUT {} を読めない", path.display())))?;
        let mut guard = self
            .loaded
            .lock()
            .map_err(|_| MmError::Internal("LUT のキャッシュが壊れた".into()))?;
        if let Some(l) = guard
            .as_ref()
            .filter(|l| l.path == path && l.digest == digest)
        {
            return Ok(l.lut.clone());
        }
        let lut = Arc::new(Lut::parse_cube(&std::fs::read_to_string(&path)?)?);
        *guard = Some(Loaded {
            path,
            digest,
            lut: lut.clone(),
        });
        Ok(lut)
    }
}

const LUT_BODY_WGSL: &str = r"
fn mm_encode(c: vec3<f32>) -> vec3<f32> {
    if (u.d.x == 1.0) {
        return mm_linear_to_srgb3(c);
    }
    if (u.d.x == 2.0) {
        return mm_linear_to_bt1886_3(c);
    }
    return c;
}

fn mm_decode(c: vec3<f32>) -> vec3<f32> {
    if (u.d.x == 1.0) {
        return mm_srgb_to_linear3(c);
    }
    if (u.d.x == 2.0) {
        return mm_bt1886_to_linear3(c);
    }
    return c;
}

fn mm_color(c: vec3<f32>) -> vec3<f32> {
    let span = max(u.c.xyz - u.b.xyz, vec3<f32>(1e-6));
    let e = mm_encode(c);
    let inside = clamp(e, u.b.xyz, u.c.xyz);
    let looked = mm_decode(mm_lut_sample((inside - u.b.xyz) / span) + (e - inside));
    return mix(c, looked, u.a.x);
}
";

/// LUT を CPU で引く。定義域の外へ出た分（1 を超えるハイライト、広色域の負の値）は端の値に足して持ち越し、切らない。
/// GPU の `LUT_BODY_WGSL` の `mm_color` と同じ式。
fn sample_carrying(lut: &Lut, enc: [f32; 3]) -> [f32; 3] {
    let inside: [f32; 3] =
        std::array::from_fn(|i| enc[i].clamp(lut.domain_min[i], lut.domain_max[i]));
    let looked = lut.sample(inside);
    std::array::from_fn(|i| looked[i] + enc[i] - inside[i])
}

impl NodeKind for ColorLutKind {
    fn id(&self) -> &'static str {
        "color.lut"
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        ParamSchema::new(vec![
            ParamSpec::new(
                LUT_PARAM,
                ParamType::AssetRef,
                ParamValue::AssetRef(AssetId::new("")),
            ),
            ParamSpec::new(
                "intensity",
                ParamType::Number {
                    min: Some(0.0),
                    max: Some(1.0),
                },
                ParamValue::Number(1.0),
            ),
            ParamSpec::new(
                "input_space",
                ParamType::Choice {
                    options: SPACES.map(String::from).to_vec(),
                },
                ParamValue::Choice("bt709".into()),
            ),
        ])
    }

    fn inputs(&self) -> InputSchema {
        input_schema()
    }

    fn traits(&self) -> Traits {
        Traits::PIXEL
    }

    /// 定義域の外の値を端の値に足して持ち越す作り方のバージョンと、LUT のファイルの中身のハッシュ。
    /// 画素の作り方を変えたらバージョンを上げる。LUT は取り込んだ後に書き換えられうる（`contentHash` は取り込み時の値）。
    fn cache_fingerprint(
        &self,
        params: &Params,
        asset_path: &dyn Fn(&AssetId) -> Option<PathBuf>,
    ) -> Option<String> {
        match lut_asset(params) {
            Some(id) => Some(format!(
                "{LUT_PIXEL_VERSION}|{}",
                file_digest(&asset_path(id)?)?
            )),
            None => Some(LUT_PIXEL_VERSION.into()),
        }
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        migrate_same_version(self, old, from)
    }

    fn is_identity(&self, cx: &FrameCx<'_>) -> bool {
        lut_is_identity(cx.params)
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        if lut_is_identity(cx.params) {
            return source_frame(cx);
        }
        let lut = self.load(&lut_path(cx)?)?;
        let intensity = cx.params.number_or("intensity", 1.0).clamp(0.0, 1.0) as f32;
        let encoding = Encoding::of(cx.params);
        let mut uniform = [0.0; 16];
        uniform[0] = intensity;
        uniform[4..7].copy_from_slice(&lut.domain_min);
        uniform[8..11].copy_from_slice(&lut.domain_max);
        uniform[12] = encoding.code();
        let cpu_lut = lut.clone();
        let pass = ColorPass {
            wgsl: format!("{}\n{LUT_WGSL}\n{LUT_BODY_WGSL}", transfer::TRANSFER_WGSL),
            uniform,
            lut: Some(lut),
            cpu: Box::new(move |rgb| {
                let enc = rgb.map(|c| encoding.encode(c));
                let looked = sample_carrying(&cpu_lut, enc).map(|c| encoding.decode(c));
                std::array::from_fn(|i| rgb[i] + (looked[i] - rgb[i]) * intensity)
            }),
        };
        apply(cx, &self.cache, &pass)
    }
}

/// LUT の素材。`AssetRef` が無いか空なら `None`。
fn lut_asset(params: &Params) -> Option<&AssetId> {
    params
        .get(LUT_PARAM)
        .and_then(ParamValue::as_asset)
        .filter(|id| !id.as_str().is_empty())
}

/// LUT の素材のファイル。素材をパスに直すのは、プロジェクトのデータを持つ呼び出し側（`InputFrames::asset_path`）。
fn lut_path(cx: &FrameCx<'_>) -> Result<PathBuf> {
    let id =
        lut_asset(cx.params).ok_or_else(|| MmError::InvalidArgument("LUT の素材が無い".into()))?;
    cx.inputs
        .asset_path(id)
        .ok_or_else(|| MmError::NotFound(format!("LUT の素材 {id}")))
}

/// 素材が無いか、強さが 0 なら何もしない。
fn lut_is_identity(params: &Params) -> bool {
    lut_asset(params).is_none() || params.number_or("intensity", 1.0) <= 0.0
}

/// `ColorTransform::program` の結果をシェーダーの形にしたもの。
struct Program {
    wgsl: String,
    lut: Option<Arc<Lut>>,
}

/// 直近に作った変換。組が変わらない間は `program` を呼び直さない。
struct Converted {
    from: ColorSpace,
    to: ColorSpace,
    program: Option<Arc<Program>>,
}

/// 色空間の変換。入力の値を `from` の符号化値として読み、`to` の符号化値にして返す。
#[derive(Default)]
pub struct ColorConvertKind {
    cache: PassCache,
    converted: Mutex<Option<Converted>>,
}

impl std::fmt::Debug for ColorConvertKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ColorConvertKind")
    }
}

fn space(params: &Params, name: &str) -> ColorSpace {
    ColorSpace(
        params
            .get(name)
            .and_then(ParamValue::as_text)
            .unwrap_or("lin_rec709")
            .to_string(),
    )
}

fn shader_program(program: TransformProgram) -> Result<Option<Program>> {
    Ok(match program {
        TransformProgram::Identity => None,
        TransformProgram::Wgsl { source } => Some(Program {
            wgsl: format!("{source}\nfn mm_color(c: vec3<f32>) -> vec3<f32> {{ return mm_color_transform(c); }}\n"),
            lut: None,
        }),
        TransformProgram::Lut3d { size, data, shaper } => Some(Program {
            wgsl: format!(
                "{}\n{LUT_WGSL}\nfn mm_color(c: vec3<f32>) -> vec3<f32> {{ return mm_lut_sample({}); }}\n",
                transfer::TRANSFER_WGSL,
                shaper.wgsl("c")
            ),
            lut: Some(Arc::new(Lut::new(size, data, [0.0; 3], [1.0; 3])?)),
        }),
    })
}

impl ColorConvertKind {
    fn program(
        &self,
        color: &dyn ColorTransform,
        from: &ColorSpace,
        to: &ColorSpace,
    ) -> Result<Option<Arc<Program>>> {
        let mut guard = self
            .converted
            .lock()
            .map_err(|_| MmError::Internal("変換のキャッシュが壊れた".into()))?;
        if let Some(c) = guard.as_ref().filter(|c| &c.from == from && &c.to == to) {
            return Ok(c.program.clone());
        }
        let program = shader_program(color.program(from, to)?)?.map(Arc::new);
        *guard = Some(Converted {
            from: from.clone(),
            to: to.clone(),
            program: program.clone(),
        });
        Ok(program)
    }
}

impl NodeKind for ColorConvertKind {
    fn id(&self) -> &'static str {
        "color.convert"
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        let name = || ParamValue::Text("lin_rec709".into());
        ParamSchema::new(vec![
            ParamSpec::new("from", ParamType::Text, name()),
            ParamSpec::new("to", ParamType::Text, name()),
        ])
    }

    fn inputs(&self) -> InputSchema {
        input_schema()
    }

    fn traits(&self) -> Traits {
        Traits::PIXEL
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        migrate_same_version(self, old, from)
    }

    fn is_identity(&self, cx: &FrameCx<'_>) -> bool {
        space(cx.params, "from") == space(cx.params, "to")
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let (from, to) = (space(cx.params, "from"), space(cx.params, "to"));
        if from == to {
            return source_frame(cx);
        }
        let color = cx.services.color.clone();
        let Some(program) = self.program(color.as_ref(), &from, &to)? else {
            return source_frame(cx);
        };
        // 知らない色空間はここで `Err` にし、画素ごとの式は失敗しない前提にする。
        color.apply(&from, &to, [0.0; 3])?;
        let pass = ColorPass {
            wgsl: program.wgsl.clone(),
            uniform: [0.0; 16],
            lut: program.lut.clone(),
            cpu: Box::new(move |rgb| color.apply(&from, &to, rgb).unwrap_or(rgb)),
        };
        apply(cx, &self.cache, &pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 画素の作り方のバージョンをフレームのキャッシュの指紋に出す() {
        let fp = ColorLutKind::default().cache_fingerprint(&Params::new(), &|_| None);
        assert_eq!(fp.as_deref(), Some(LUT_PIXEL_VERSION));
    }

    #[test]
    fn lut_のファイルを書き換えると指紋が変わり_読めなければキャッシュしない() {
        let dir = std::env::temp_dir().join(format!("mm-color-lut-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("一時フォルダー");
        let path = dir.join("a.cube");
        let mut params = Params::new();
        params.set(LUT_PARAM, ParamValue::AssetRef(AssetId::new("lut")));
        let kind = ColorLutKind::default();
        let fp = |p: &Path| kind.cache_fingerprint(&params, &|_| Some(p.to_path_buf()));
        std::fs::write(&path, "LUT_3D_SIZE 2\n").expect("書ける");
        let first = fp(&path).expect("読める");
        assert!(first.starts_with(LUT_PIXEL_VERSION));
        std::fs::write(&path, "LUT_3D_SIZE 3\n").expect("書ける");
        assert_ne!(fp(&path), Some(first), "中身が変われば別の指紋");
        assert_eq!(fp(&dir.join("none.cube")), None);
    }

    #[test]
    fn 更新時刻の刻みの中で書き換えた_lut_も読み直す() {
        let dir = std::env::temp_dir().join(format!("mm-color-lut-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("一時フォルダー");
        let path = dir.join("same-time.cube");
        // 2 回の書き込みが同じ刻みに入った形。更新の直後なので、中身のハッシュは読み直して作る。
        let at = std::time::SystemTime::now();
        let write = |domain: &str| {
            let body = format!(
                "LUT_3D_SIZE 2\nDOMAIN_MAX {domain}\n{}",
                "0 0 0\n".repeat(8)
            );
            std::fs::write(&path, body).expect("書ける");
            let file = std::fs::File::options().write(true).open(&path);
            file.and_then(|f| f.set_modified(at))
                .expect("更新時刻を戻せる");
        };
        let kind = ColorLutKind::default();
        write("1 1 1");
        assert_eq!(kind.load(&path).expect("読める").domain_max, [1.0; 3]);
        write("2 2 2");
        assert_eq!(kind.load(&path).expect("読める").domain_max, [2.0; 3]);
    }
}
