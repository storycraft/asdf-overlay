use core::{num::NonZeroUsize, ptr, u32};

use anyhow::Context;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIKeyedMutex, IDXGIResource,
            },
        },
    },
    core::Interface,
};

const DEFAULT_FEATURE_LEVELS: [D3D_FEATURE_LEVEL; 1] = [D3D_FEATURE_LEVEL_11_0];

pub struct OverlaySurface {
    device: ID3D11Device,
    cx: ID3D11DeviceContext,

    texture: Option<ID3D11Texture2D>,
}

impl OverlaySurface {
    pub fn new() -> anyhow::Result<Self> {
        let mut device = None;
        let mut cx = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&DEFAULT_FEATURE_LEVELS),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )?;
        }
        let device = device.context("failed to create Dx11 Device")?;
        let cx = cx.context("failed to create Dx11 Context")?;

        Ok(Self {
            device,
            cx,
            texture: None,
        })
    }

    fn desc(&self) -> D3D11_TEXTURE2D_DESC {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        let Some(ref texture) = self.texture else {
            return desc;
        };

        unsafe { texture.GetDesc(&mut desc) };
        desc
    }

    fn size(&self) -> (u32, u32) {
        let desc = self.desc();
        (desc.Width, desc.Height)
    }

    pub fn update_bitmap(
        &mut self,
        width: u32,
        data: &[u8],
    ) -> anyhow::Result<Option<NonZeroUsize>> {
        if width == 0 || data.is_empty() {
            self.texture = None;
            return Ok(None);
        }

        let size = (width, (data.len() / width as usize / 4) as u32);
        let prev_size = self.size();
        if prev_size.0 != size.0 || prev_size.1 != size.1 {
            self.texture = None;
        }

        let row_pitch = width * 4;

        match self.texture {
            Some(ref texture) => {
                let mutex = texture.cast::<IDXGIKeyedMutex>()?;
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });
                    self.cx
                        .UpdateSubresource(texture, 0, None, data.as_ptr().cast(), row_pitch, 0);
                    self.cx.Flush();
                }

                Ok(None)
            }

            None => {
                let mut texture = None;

                unsafe {
                    self.device.CreateTexture2D(
                        &D3D11_TEXTURE2D_DESC {
                            Width: size.0,
                            Height: size.1,
                            MipLevels: 1,
                            ArraySize: 1,
                            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Usage: D3D11_USAGE_DEFAULT,
                            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as _,
                            CPUAccessFlags: 0,
                            MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
                        },
                        Some(&D3D11_SUBRESOURCE_DATA {
                            pSysMem: data.as_ptr().cast(),
                            SysMemPitch: row_pitch,
                            SysMemSlicePitch: 0,
                        }),
                        Some(&mut texture),
                    )?;
                    let texture = self
                        .texture
                        .insert(texture.context("cannot create texture")?);

                    Ok(NonZeroUsize::new(
                        texture.cast::<IDXGIResource>()?.GetSharedHandle()?.0 as usize,
                    ))
                }
            }
        }
    }
}
