//! `ofx` の種類。

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use mm_host_api::{HostLauncher, HostRequest, SharedFrame};
use mm_node_api::{
    Cost, FrameCx, FrameRef, InputFrames, InputSchema, InputSpec, Isolation, NodeKind, Traits,
};
use mm_types::{
    AssetId, MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, RgbaImage,
};

use crate::catalog::{Catalog, Describe, OfxPlugin};
use crate::hosts::{Hosts, Target};
use crate::locate::{resolve_asset, resolve_named};
use crate::{ASSET_PARAM, BUNDLE_PARAM, KIND_ID, KIND_VERSION, PLUGIN_PARAM};

/// OpenFX のプラグインで入力 0 のレイヤーの画像を描き直す種類（Filter の文脈。入力が無ければ Generator として透明を渡す）。
pub struct OfxKind {
    hosts: Mutex<Hosts>,
    catalog: Catalog,
    /// バンドルを探してよい置き場所。
    roots: Vec<PathBuf>,
}

/// パラメータが指すバンドルの指し方。
enum BundleRef<'a> {
    Named(&'a str),
    Asset(&'a AssetId),
}

impl OfxKind {
    /// `executable`（`mm-host-ofx`）を `launcher` で起こして描く。`describe` でバンドルの効果を引き、
    /// バンドルは `roots`（OpenFX の置き場所）の中かプロジェクトの素材だけを受ける。
    pub fn new(
        launcher: Arc<dyn HostLauncher>,
        executable: PathBuf,
        describe: Describe,
        roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            hosts: Mutex::new(Hosts::new(launcher, executable)),
            catalog: Catalog::new(describe),
            roots,
        }
    }

    /// ホストの組のロック。描画の途中でパニックしたホストは応答の途中にいるかもしれないので、
    /// 毒の入ったロックは全部のホストを捨ててから使う。
    fn hosts(&self) -> MutexGuard<'_, Hosts> {
        self.hosts.lock().unwrap_or_else(|poisoned| {
            let mut hosts = poisoned.into_inner();
            hosts.clear();
            self.hosts.clear_poison();
            hosts
        })
    }

    /// パラメータが指すバンドルの実体のパス。素材を引けない呼び出し（`params_for`）では `inputs` が `None`。
    fn bundle_path(
        &self,
        params: &Params,
        inputs: Option<&dyn InputFrames>,
    ) -> Result<Option<PathBuf>> {
        let path = match bundle_ref(params)? {
            None => return Ok(None),
            Some(BundleRef::Named(name)) => resolve_named(&self.roots, name)?,
            Some(BundleRef::Asset(id)) => {
                let path = inputs.and_then(|i| i.asset_path(id)).ok_or_else(|| {
                    MmError::NotFound(format!("素材 {} のパスを引けない", id.as_str()))
                })?;
                resolve_asset(&path)?
            }
        };
        Ok(Some(path))
    }

    /// パラメータが指す効果とホストの読み込み先。何も指していなければ `None`。
    fn target(
        &self,
        params: &Params,
        inputs: Option<&dyn InputFrames>,
    ) -> Result<Option<(Target, OfxPlugin)>> {
        let Some(bundle) = self.bundle_path(params, inputs)? else {
            return Ok(None);
        };
        let id = text(params, PLUGIN_PARAM);
        let (plugin, stamp) = self.catalog.plugin(&bundle, id)?;
        let target = Target {
            bundle,
            plugin: plugin.id.clone(),
            stamp,
        };
        Ok(Some((target, plugin)))
    }
}

fn text<'a>(params: &'a Params, name: &str) -> &'a str {
    params
        .get(name)
        .and_then(ParamValue::as_text)
        .unwrap_or_default()
}

/// `bundle` と `asset` のどちらで指しているか。どちらも空なら `None`、両方なら誤り。
fn bundle_ref(params: &Params) -> Result<Option<BundleRef<'_>>> {
    let name = text(params, BUNDLE_PARAM);
    let asset = params
        .get(ASSET_PARAM)
        .and_then(ParamValue::as_asset)
        .filter(|id| !id.as_str().is_empty());
    match (name.is_empty(), asset) {
        (true, None) => Ok(None),
        (false, None) => Ok(Some(BundleRef::Named(name))),
        (true, Some(id)) => Ok(Some(BundleRef::Asset(id))),
        (false, Some(_)) => Err(MmError::InvalidArgument(format!(
            "{BUNDLE_PARAM} と {ASSET_PARAM} は片方だけで指す"
        ))),
    }
}

/// 種類の自前のパラメータの名前か。プラグインの同じ名前のパラメータは載せず、値も渡さない。
fn is_own(name: &str) -> bool {
    [BUNDLE_PARAM, ASSET_PARAM, PLUGIN_PARAM].contains(&name)
}

/// ホストに渡す値。プラグインが持つ名前だけを渡し、ほかの名前（前に選んでいたプラグインの値など）は捨てる。
fn plugin_values(params: &Params, plugin: &OfxPlugin) -> Params {
    let mut values = Params::new();
    for (name, value) in &params.0 {
        if !is_own(name) && plugin.settable.contains(name) {
            values.set(name.clone(), value.clone());
        }
    }
    values
}

/// 入力 0 の画像。入力が無ければ `None`（ホストは透明を渡す）。
fn source_image(cx: &mut FrameCx<'_>) -> Result<Option<Arc<RgbaImage>>> {
    if cx.inputs.is_empty() {
        return Ok(None);
    }
    let frame = cx.inputs.frame(0, cx.time)?;
    let image = cx.gpu.download(&frame)?;
    if image.size() != cx.size {
        return Err(MmError::Internal(format!(
            "入力の大きさ {:?} が出力の {:?} と違う",
            image.size(),
            cx.size
        )));
    }
    Ok(Some(image))
}

/// ホストの応答を画像にする。CPU の画像だけを頼むので、ほかは誤り。
fn response_image(frame: SharedFrame, cx: &FrameCx<'_>) -> Result<RgbaImage> {
    let image = match frame {
        SharedFrame::Cpu { size, data } => RgbaImage::from_raw(size, data)?,
        SharedFrame::SharedTexture { .. } => {
            return Err(MmError::Internal(
                "CPU の画像を頼んだのに共有テクスチャが来た".into(),
            ))
        }
    };
    if image.size() != cx.size {
        return Err(MmError::Internal(format!(
            "ofx のフレームの大きさ {:?} が頼んだ {:?} と違う",
            image.size(),
            cx.size
        )));
    }
    Ok(image)
}

impl NodeKind for OfxKind {
    fn id(&self) -> &'static str {
        KIND_ID
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        let empty = || ParamValue::Text(String::new());
        ParamSchema::new(vec![
            ParamSpec::new(BUNDLE_PARAM, ParamType::Text, empty()),
            ParamSpec::new(
                ASSET_PARAM,
                ParamType::AssetRef,
                ParamValue::AssetRef(AssetId::new("")),
            ),
            ParamSpec::new(PLUGIN_PARAM, ParamType::Text, empty()),
        ])
    }

    /// 選んだ効果のパラメータを後ろに足す。素材のバンドルはここでは素材のパスを引けないので、自前の分だけ返す。
    fn params_for(&self, params: &Params) -> ParamSchema {
        let mut schema = self.params();
        match self.target(params, None) {
            Ok(Some((_, plugin))) => schema.params.extend(
                plugin
                    .schema
                    .params
                    .into_iter()
                    .filter(|p| !is_own(&p.name)),
            ),
            Ok(None) => {}
            Err(e) => tracing::debug!(%e, "ofx のパラメータの一覧を引けない"),
        }
        schema
    }

    fn inputs(&self) -> InputSchema {
        InputSchema {
            inputs: vec![InputSpec {
                name: "source".into(),
                optional: true,
            }],
        }
    }

    /// プラグインは時刻を読めるので時間で変わると申告し、同じ入力なら同じ画素と見なす。
    fn traits(&self) -> Traits {
        Traits {
            realtime: false,
            cost: Cost::Heavy,
            isolation: Isolation::OutOfProcess,
            ..Traits::PIXEL
        }
    }

    /// 自前のパラメータの型だけを確かめる。プラグインのパラメータは選んだプラグインで決まり、ここでは読み込まない。
    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if from != KIND_VERSION {
            return Err(MmError::Unsupported(format!(
                "{KIND_ID} のバージョン {from} からの移行が無い"
            )));
        }
        let params: Params = serde_json::from_value(old)?;
        for spec in &self.params().params {
            if params.get(&spec.name).is_some_and(|v| !v.matches(&spec.ty)) {
                return Err(MmError::InvalidArgument(format!(
                    "パラメータ {} の型が合わない",
                    spec.name
                )));
            }
        }
        Ok(params)
    }

    /// プラグインを指していなければ入力をそのまま返す。
    fn is_identity(&self, cx: &FrameCx<'_>) -> bool {
        matches!(bundle_ref(cx.params), Ok(None))
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let _span = tracing::info_span!("nodes_ofx.render").entered();
        let target = self.target(cx.params, Some(&*cx.inputs))?;
        let Some((target, plugin)) = target else {
            if cx.inputs.is_empty() {
                return Ok(FrameRef::Transparent { size: cx.size });
            }
            return cx.inputs.frame(0, cx.time);
        };
        if cx.size.area() == 0 {
            return Ok(FrameRef::Transparent { size: cx.size });
        }
        let source = source_image(cx)?;
        let request = HostRequest::RenderFrame {
            time: cx.time,
            size: cx.size,
            params: serde_json::json!({
                "source": source.map(|image| SharedFrame::from_image(&image)),
                "values": plugin_values(cx.params, &plugin),
            }),
        };
        let frame = self.hosts().render(&target, &request)?;
        Ok(FrameRef::cpu(response_image(frame, cx)?))
    }
}

impl std::fmt::Debug for OfxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfxKind")
            .field("hosts", &*self.hosts())
            .field("catalog", &self.catalog)
            .field("roots", &self.roots)
            .finish()
    }
}
