//! アプリ側としてホストの共有テクスチャを開く。結合テストと、往復を測る例（`examples/bench_roundtrip.rs`）が共有する。
#![cfg(all(feature = "cef", windows))]

use mm_gpu::GpuContext;
use mm_types::Size;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

/// アプリ側として共有ハンドルを開いて wgpu のテクスチャにし、ハンドルを閉じる。
pub(crate) fn open_shared(ctx: &GpuContext, handle: u64, size: Size) -> wgpu::Texture {
    let handle = HANDLE(handle as usize as *mut _);
    // SAFETY: as_hal の参照はこのブロックの間だけ使う。handle はホストがこのプロセスへ複製したもの。
    let resource = unsafe {
        let hal = ctx.device.as_hal::<wgpu::hal::api::Dx12>().expect("D3D12");
        let mut resource: Option<ID3D12Resource> = None;
        hal.raw_device()
            .OpenSharedHandle(handle, &mut resource)
            .expect("共有ハンドルを開ける");
        CloseHandle(handle).expect("ハンドルを閉じられる");
        resource.expect("資源")
    };
    let extent = wgpu::Extent3d {
        width: size.width,
        height: size.height,
        depth_or_array_layers: 1,
    };
    let desc = wgpu::TextureDescriptor {
        label: Some("host frame"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    // SAFETY: ホストは BGRA8 の 2D テクスチャを COMMON で渡す。
    unsafe {
        let hal = wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            desc.format,
            desc.dimension,
            extent,
            1,
            1,
        );
        ctx.device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
            hal,
            &desc,
            wgpu::TextureUses::PRESENT,
        )
    }
}
