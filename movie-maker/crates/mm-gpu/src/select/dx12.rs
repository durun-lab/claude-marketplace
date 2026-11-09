//! D3D12 の GPU を、DXGI の一覧だけで選ぶ。
//!
//! wgpu の列挙は全部のアダプターに D3D12 のデバイスを作り、内蔵 GPU では 1 つに約 1 秒かかる。
//! DXGI の記述（名前と LUID）はデバイスなしで読めるので、選んだ 1 つだけを wgpu-hal に渡す。

use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::IDXGIAdapter1;

use super::AdapterCandidate;

/// DXGI の LUID を 64 ビットの値にする（上位が HighPart）。
pub(crate) fn luid_value(luid: LUID) -> u64 {
    ((luid.HighPart as u32 as u64) << 32) | u64::from(luid.LowPart)
}

/// DXGI の列挙の順のアダプターと、その記述。
pub(super) struct Listing {
    adapters: Vec<IDXGIAdapter1>,
    pub(super) candidates: Vec<AdapterCandidate>,
}

/// `instance` の D3D12 のアダプターを、デバイスを作らずに並べる。D3D12 が有効でなければ `None`。
pub(super) fn list(instance: &wgpu::Instance) -> Option<Listing> {
    // SAFETY: hal の Instance は `instance` より長く使わず、DXGI の列挙と記述の読み取りだけを行う。
    let hal = unsafe { instance.as_hal::<wgpu::hal::api::Dx12>() }?;
    let factory = unsafe { hal.raw_factory4() };
    let mut adapters = Vec::new();
    let mut candidates = Vec::new();
    // 列挙の終わり（DXGI_ERROR_NOT_FOUND）とほかの失敗のどちらでも、そこまでの一覧で選ぶ。
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapters.len() as u32) } {
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            break;
        };
        let len = desc.Description.iter().position(|&c| c == 0);
        let name =
            String::from_utf16_lossy(&desc.Description[..len.unwrap_or(desc.Description.len())]);
        candidates.push(AdapterCandidate {
            name,
            luid: Some(luid_value(desc.AdapterLuid)),
        });
        adapters.push(adapter);
    }
    Some(Listing {
        adapters,
        candidates,
    })
}

/// `listing` の `index` 番だけを wgpu のアダプターにする。D3D12 で使えなければ `None`。
pub(super) fn expose(
    instance: &wgpu::Instance,
    listing: &Listing,
    index: usize,
) -> Option<wgpu::Adapter> {
    // SAFETY: hal の Instance は `instance` のもので、作ったアダプターは同じ `instance` に渡す。
    let hal = unsafe { instance.as_hal::<wgpu::hal::api::Dx12>() }?;
    let exposed = hal.expose_adapter(listing.adapters.get(index)?)?;
    Some(unsafe { instance.create_adapter_from_hal(exposed) })
}
