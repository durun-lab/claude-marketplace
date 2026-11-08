//! `GpuSelector` で wgpu のアダプターを選ぶ。GPU を作る所は全部ここを通り、`MM_GPU` に従う。

use mm_types::{GpuAdapterId, MmError, Result};

use super::{choose, AdapterCandidate, GpuSelector, GPU_ENV};
use crate::context::{adapter_id, default_backends, luid_of};

/// CLI やホストの引数で選ぶときの、エラーに出す出どころ。
const PATTERN_LABEL: &str = "GPU の指定";

/// `instance` のアダプターから `selector` に合う 1 つを選ぶ。Windows で D3D12 が有効なら、
/// DXGI の一覧から選んだ 1 つにだけデバイスを作る。ほかのバックエンドは wgpu の列挙から選ぶ。
/// `label` は合わないときのエラーに出す指定の出どころ（`MM_GPU` など）。
pub async fn find_adapter(
    instance: &wgpu::Instance,
    label: &str,
    selector: &GpuSelector,
) -> Result<wgpu::Adapter> {
    #[cfg(windows)]
    if let Some(listing) = super::dx12::list(instance) {
        let index = choose(label, selector, &listing.candidates)?;
        return super::dx12::expose(instance, &listing, index).ok_or_else(|| {
            MmError::Unsupported(format!(
                "GPU {} は D3D12 で使えない",
                listing.candidates[index].name
            ))
        });
    }
    let mut all = instance.enumerate_adapters(wgpu::Backends::all()).await;
    let candidates: Vec<AdapterCandidate> = all
        .iter()
        .map(|a| AdapterCandidate {
            name: a.get_info().name,
            luid: luid_of(a),
        })
        .collect();
    let index = choose(label, selector, &candidates)?;
    Ok(all.swap_remove(index))
}

/// GPU のコンテキストに使うアダプター。`MM_GPU` があればそれ、無ければ `wanted`（設定で選んだ GPU）、
/// どちらも無ければ高性能のアダプター。`MM_GPU` と `wanted` が食い違えば、黙ってどちらかに寄せずエラー。
pub async fn select_adapter(
    instance: &wgpu::Instance,
    wanted: Option<&GpuAdapterId>,
) -> Result<wgpu::Adapter> {
    if let Some(selector) = GpuSelector::from_env()? {
        let adapter = find_adapter(instance, GPU_ENV, &selector).await?;
        let id = adapter_id(&adapter);
        return match wanted {
            Some(w) if !id.matches(w) => Err(MmError::InvalidArgument(format!(
                "GPU {} を求められたが、{GPU_ENV}={selector} で {} だけを使う",
                w.name, id.name
            ))),
            _ => Ok(adapter),
        };
    }
    match wanted {
        Some(w) => pick_wanted(instance, w).await,
        None => instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .map_err(|e| MmError::Unsupported(format!("GPU のアダプターが無い: {e}"))),
    }
}

/// 設定で選んだ GPU。同じ回の LUID が合えばその 1 つだけを開き、LUID が古ければ機種で探す。
async fn pick_wanted(instance: &wgpu::Instance, wanted: &GpuAdapterId) -> Result<wgpu::Adapter> {
    if let Some(luid) = wanted.luid {
        let exact = find_adapter(instance, "保存した GPU", &GpuSelector::Luid(luid)).await;
        if let Some(adapter) = exact.ok().filter(|a| adapter_id(a).matches(wanted)) {
            return Ok(adapter);
        }
    }
    let mut same_model: Vec<_> = instance
        .enumerate_adapters(wgpu::Backends::all())
        .await
        .into_iter()
        .filter(|a| adapter_id(a).matches(wanted))
        .collect();
    let exact = same_model
        .iter()
        .position(|a| adapter_id(a).is_same_instance(wanted))
        .unwrap_or(0);
    if same_model.is_empty() {
        Err(MmError::Unsupported(format!("GPU {} が無い", wanted.name)))
    } else {
        Ok(same_model.swap_remove(exact))
    }
}

/// このプロセスで使える GPU の一覧。設定で選ぶときの候補。`MM_GPU` があれば合う 1 つだけ。
pub fn enumerate_adapters() -> Result<Vec<GpuAdapterId>> {
    let instance = default_instance();
    match GpuSelector::from_env()? {
        Some(selector) => {
            let adapter = pollster::block_on(find_adapter(&instance, GPU_ENV, &selector))?;
            Ok(vec![adapter_id(&adapter)])
        }
        None => Ok(
            pollster::block_on(instance.enumerate_adapters(default_backends()))
                .iter()
                .map(adapter_id)
                .collect(),
        ),
    }
}

/// CLI やホストの引数で GPU を選ぶ。書き方は `MM_GPU` と同じで、`MM_GPU` で見える GPU の中から選ぶ。
pub fn find_adapter_id(pattern: &str) -> Result<GpuAdapterId> {
    let selector = GpuSelector::parse(pattern)?
        .ok_or_else(|| MmError::InvalidArgument("GPU の指定が空".into()))?;
    let visible = enumerate_adapters()?;
    let candidates: Vec<AdapterCandidate> = visible
        .iter()
        .map(|a| AdapterCandidate {
            name: a.name.clone(),
            luid: a.luid,
        })
        .collect();
    let index = choose(PATTERN_LABEL, &selector, &candidates)?;
    Ok(visible[index].clone())
}

fn default_instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = default_backends();
    wgpu::Instance::new(desc)
}
