//! ホストがこのプロセスへ複製した D3D12 の共有テクスチャの NT ハンドルを開き、wgpu のテクスチャにする。
//! ハンドルは開いたらすぐ閉じる（資源はテクスチャが持つ）。形式は mm-host-cef の約束（BGRA8、COMMON）。

use mm_gpu::GpuContext;
use mm_types::{MmError, Result, Size};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D12::{
    ID3D12Resource, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

/// 共有テクスチャの形式。
pub const SHARED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// 開いたテクスチャ。コピーの元にだけ使う。コピーの後は状態を `PRESENT`（COMMON）に戻す。
pub fn open(ctx: &GpuContext, handle: u64, size: Size) -> Result<wgpu::Texture> {
    let resource = open_resource(ctx, handle)?;
    // SAFETY: 開いたばかりの資源の記述を読むだけ。
    check_desc(&unsafe { resource.GetDesc() }, size)?;
    let extent = wgpu::Extent3d {
        width: size.width,
        height: size.height,
        depth_or_array_layers: 1,
    };
    let desc = wgpu::TextureDescriptor {
        label: Some("web.cef shared"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHARED_FORMAT,
        usage: wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    // SAFETY: 記述は check_desc で BGRA8 の 2D、頼んだ大きさ、1 枚と確かめた。ホストは COMMON の状態で渡し、渡した後は書かない。
    let texture = unsafe {
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
    };
    Ok(texture)
}

fn open_resource(ctx: &GpuContext, handle: u64) -> Result<ID3D12Resource> {
    let raw = HANDLE(handle as usize as *mut _);
    // SAFETY: as_hal の参照はこのブロックの間だけ使う。raw はホストがこのプロセスへ複製したハンドルで、ほかに使う所は無い。
    unsafe {
        let opened = match ctx.device.as_hal::<wgpu::hal::api::Dx12>() {
            Some(hal) => {
                let mut resource: Option<ID3D12Resource> = None;
                hal.raw_device()
                    .OpenSharedHandle(raw, &mut resource)
                    .map(|()| resource)
                    .map_err(|e| format!("共有ハンドルを開けない: {e}"))
            }
            None => Err("D3D12 のデバイスでない".to_string()),
        };
        let _ = CloseHandle(raw);
        opened
            .map_err(MmError::Unsupported)?
            .ok_or_else(|| MmError::Internal("共有ハンドルの資源が空".into()))
    }
}

/// 資源の記述が `texture_from_raw` に渡す形（BGRA8 の 2D、`size`、ミップと配列とサンプルが 1）と合うか。
/// 合わない資源を wgpu のテクスチャにすると、コピーが資源の外を読む。
fn check_desc(desc: &D3D12_RESOURCE_DESC, size: Size) -> Result<()> {
    let fits = desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D
        && desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM
        && desc.Width == u64::from(size.width)
        && desc.Height == size.height
        && desc.DepthOrArraySize == 1
        && desc.MipLevels == 1
        && desc.SampleDesc.Count == 1;
    if fits {
        return Ok(());
    }
    Err(MmError::Unsupported(format!(
        "共有テクスチャの資源が約束と違う: {:?} {:?} {}x{}（頼んだのは BGRA8 の 2D {}x{}）",
        desc.Dimension, desc.Format, desc.Width, desc.Height, size.width, size.height
    )))
}

/// 使わずに捨てる応答のハンドルを閉じる。
pub fn close(handle: u64) {
    // SAFETY: ホストがこのプロセスへ複製したハンドルで、ほかに使う所は無い。
    let _ = unsafe { CloseHandle(HANDLE(handle as usize as *mut _)) };
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_DIMENSION_BUFFER;
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

    use super::*;

    const SIZE: Size = Size::new(64, 36);

    fn desc() -> D3D12_RESOURCE_DESC {
        D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(SIZE.width),
            Height: SIZE.height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        }
    }

    #[test]
    fn 約束どおりの資源は通す() {
        assert!(check_desc(&desc(), SIZE).is_ok());
    }

    #[test]
    fn 大きさと形式と次元が違う資源は誤りにする() {
        let cases = [
            D3D12_RESOURCE_DESC {
                Width: u64::from(SIZE.width) * 2,
                ..desc()
            },
            D3D12_RESOURCE_DESC {
                Height: 1,
                ..desc()
            },
            D3D12_RESOURCE_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..desc()
            },
            D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                ..desc()
            },
            D3D12_RESOURCE_DESC {
                DepthOrArraySize: 2,
                ..desc()
            },
        ];
        for case in cases {
            let err = check_desc(&case, SIZE).unwrap_err();
            assert!(matches!(err, MmError::Unsupported(_)), "{err:?}");
        }
    }
}
