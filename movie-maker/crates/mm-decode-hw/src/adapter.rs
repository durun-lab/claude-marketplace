//! レンダラーの GPU のコンテキストを D3D12 のアダプターで作る頼み方。
//!
//! `GpuContext` は既定で Vulkan と D3D12 の両方から高性能のアダプターを選ぶため、Windows の NVIDIA では
//! Vulkan が選ばれうる。d3d12va と cuvid は FFmpeg と CUDA に D3D12 のデバイスを渡す（共有資源は
//! 同じ `ID3D12Device` の上に置く）ので、LUID を持つ D3D12 のアダプターを名指しして頼む。
//! アダプターは `mm_gpu` の選び方（環境変数 `MM_GPU` を含む）で選ぶ。

use mm_gpu::{adapter_id, find_adapter, select_adapter, GpuRequest, GpuSelector};
use mm_types::{GpuAdapterId, Result};

/// LUID のアダプターを指す `GpuRequest`。別プロセスで親と同じ GPU を選ぶのに使う。
/// LUID の 1 つだけを開くので、ほかの GPU にはデバイスを作らない。
pub fn dx12_request_for_luid(luid: u64, features: wgpu::Features) -> Result<GpuRequest> {
    let instance = dx12_instance();
    let adapter = pollster::block_on(find_adapter(
        &instance,
        "親の GPU",
        &GpuSelector::Luid(luid),
    ))?;
    Ok(request(adapter_id(&adapter), features))
}

/// D3D12 のアダプターを指す `GpuRequest`。`wanted` があればその機種、なければ D3D12 での高性能のアダプター。
/// `features` はレンダラーのデバイスに求める機能（`TIMESTAMP_QUERY` は常に求める）。
pub fn dx12_request(wanted: Option<&GpuAdapterId>, features: wgpu::Features) -> Result<GpuRequest> {
    let instance = dx12_instance();
    let adapter = pollster::block_on(select_adapter(&instance, wanted))?;
    Ok(request(adapter_id(&adapter), features))
}

fn dx12_instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::DX12;
    wgpu::Instance::new(desc)
}

fn request(adapter: GpuAdapterId, features: wgpu::Features) -> GpuRequest {
    GpuRequest {
        adapter: Some(adapter),
        optional_features: wgpu::Features::TIMESTAMP_QUERY | features,
        backends: wgpu::Backends::DX12,
    }
}
