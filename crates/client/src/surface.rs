use core::{num::NonZeroUsize, ptr};

use anyhow::Context;
use asdf_overlay_common::request::UpdateSharedHandle;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HANDLE, HMODULE},
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

pub struct OverlaySurface<const BUFFERS: usize = 2> {
    device: ID3D11Device,
    cx: ID3D11DeviceContext,

    texture: BufferedTexture<BUFFERS>,
}

impl<const BUFFERS: usize> OverlaySurface<BUFFERS> {
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
            texture: BufferedTexture::new(),
        })
    }

    pub fn clear(&mut self) {
        self.texture = BufferedTexture::new();
    }

    pub fn update_from_nt_shared(
        &mut self,
        handle: HANDLE,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let device1 = self.device.cast::<ID3D11Device1>()?;
        let src_texture = unsafe { device1.OpenSharedResource1::<ID3D11Texture2D>(handle)? };
        self.update_copy_from(&src_texture)
    }

    pub fn update_from_shared(
        &mut self,
        handle: HANDLE,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let mut src_texture = None;
        unsafe {
            self.device
                .OpenSharedResource::<ID3D11Texture2D>(handle, &mut src_texture)?
        };
        let src_texture = src_texture.unwrap();

        self.update_copy_from(&src_texture)
    }

    fn update_copy_from(
        &mut self,
        src_texture: &ID3D11Texture2D,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            src_texture.GetDesc(&mut desc);
        }
        let size = (desc.Width, desc.Height);
        let surface = self.texture.texture_for(size.0, size.1);

        match *surface {
            Some((ref surface, ref mutex)) => {
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    self.cx.CopyResource(surface, src_texture);
                }

                Ok(None)
            }

            None => {
                let (surface, mutex) =
                    surface.insert(create_surface_texture(&self.device, size.0, size.1, None)?);
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    self.cx.CopyResource(&*surface, src_texture);
                }

                Ok(Some(UpdateSharedHandle {
                    handle: NonZeroUsize::new(
                        unsafe { surface.cast::<IDXGIResource>()?.GetSharedHandle() }?.0 as usize,
                    ),
                }))
            }
        }
    }

    pub fn update_bitmap(
        &mut self,
        width: u32,
        data: &[u8],
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        if width == 0 || data.is_empty() {
            return Ok(Some(UpdateSharedHandle { handle: None }));
        }

        let size = (width, (data.len() / width as usize / 4) as u32);
        let surface = self.texture.texture_for(size.0, size.1);

        let row_pitch = width * 4;
        match *surface {
            Some((ref texture, ref mutex)) => {
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    self.cx
                        .UpdateSubresource(texture, 0, None, data.as_ptr().cast(), row_pitch, 0);
                }

                Ok(None)
            }

            None => {
                let texture = create_surface_texture(
                    &self.device,
                    size.0,
                    size.1,
                    Some(&D3D11_SUBRESOURCE_DATA {
                        pSysMem: data.as_ptr().cast(),
                        SysMemPitch: row_pitch,
                        SysMemSlicePitch: 0,
                    }),
                )?;

                let (ref texture, ref mutex) = *surface.insert(texture);
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    Ok(Some(UpdateSharedHandle {
                        handle: NonZeroUsize::new(
                            texture.cast::<IDXGIResource>()?.GetSharedHandle()?.0 as usize,
                        ),
                    }))
                }
            }
        }
    }
}

fn create_surface_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    initial: Option<&D3D11_SUBRESOURCE_DATA>,
) -> anyhow::Result<(ID3D11Texture2D, IDXGIKeyedMutex)> {
    let mut texture = None;
    unsafe {
        device.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
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
            initial.map(|r| r as *const _),
            Some(&mut texture),
        )?;
        let texture = texture.context("cannot create texture")?;
        let mutex = texture.cast::<IDXGIKeyedMutex>()?;

        Ok((texture, mutex))
    }
}

struct BufferedTexture<const BUFFERS: usize> {
    texture: [Option<(ID3D11Texture2D, IDXGIKeyedMutex)>; BUFFERS],
    index: usize,
}

impl<const BUFFERS: usize> BufferedTexture<BUFFERS> {
    pub fn new() -> Self {
        Self {
            texture: [const { None }; BUFFERS],
            index: 0,
        }
    }

    pub fn texture_for(
        &mut self,
        width: u32,
        height: u32,
    ) -> &mut Option<(ID3D11Texture2D, IDXGIKeyedMutex)> {
        let prev_size = if let Some((ref texture, _)) = self.texture[self.index] {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                texture.GetDesc(&mut desc);
            }

            (desc.Width, desc.Height)
        } else {
            (0, 0)
        };

        if prev_size.0 != width || prev_size.1 != height {
            self.index = (self.index + 1) % BUFFERS;
            let texture = &mut self.texture[self.index];
            texture.take();

            texture
        } else {
            &mut self.texture[self.index]
        }
    }
}
