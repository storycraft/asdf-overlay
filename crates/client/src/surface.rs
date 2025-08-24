//! Client side overlay surface management wrapper.
//! 
//! Uses Direct3D11 to manage overlay surfaces
//! and provide convenient methods to update them from bitmaps or other shared texture.

use core::{num::NonZeroU32, ptr};

use anyhow::{Context, bail};
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

use crate::ty::CopyRect;

const DEFAULT_FEATURE_LEVELS: [D3D_FEATURE_LEVEL; 1] = [D3D_FEATURE_LEVEL_11_0];

/// Represents an overlay surface.
/// 
/// This buffers multiple textures to prevent flickering when updating the surface.
/// The default buffer count is 2, but can be changed by specifying the `BUFFERS` const generic parameter.
pub struct OverlaySurface<const BUFFERS: usize = 2> {
    device: ID3D11Device,
    cx: ID3D11DeviceContext,

    texture: BufferedTexture<BUFFERS>,
}

impl<const BUFFERS: usize> OverlaySurface<BUFFERS> {
    /// Create a new [`OverlaySurface`].
    /// This will create a Direct3D11 device and context internally.
    /// * Returns error if failed to create Direct3D11 device or context.
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

    /// Clear the current surface.
    /// This will release all internal textures.
    pub fn clear(&mut self) {
        self.texture = BufferedTexture::new();
    }

    /// Update the surface from a NT handle of a Direct3D texture.
    /// * Returns [`None`]` if the update is done to an existing internal texture.
    /// * Returns [`Some`]` if a new internal texture is created, due to size change.
    /// * Returns error if handle is invalid to be opened.
    pub fn update_from_nt_shared(
        &mut self,
        width: u32,
        height: u32,
        handle: u32,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let device1 = self.device.cast::<ID3D11Device1>()?;
        let src_texture =
            unsafe { device1.OpenSharedResource1::<ID3D11Texture2D>(HANDLE(handle as _))? };
        self.update_surface_from(width, height, &src_texture, rect)
    }

    /// Update the surface from a KMT handle of a Direct3D texture.
    /// * Returns [`None`]` if the update is done to an existing internal texture.
    /// * Returns [`Some`]` if a new internal texture is created, due to size change.
    /// * Returns error if handle is invalid to be opened.
    pub fn update_from_shared(
        &mut self,
        width: u32,
        height: u32,
        handle: u32,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let mut src_texture = None;
        unsafe {
            self.device
                .OpenSharedResource::<ID3D11Texture2D>(HANDLE(handle as _), &mut src_texture)?
        };
        let src_texture = src_texture.unwrap();

        self.update_surface_from(width, height, &src_texture, rect)
    }

    fn update_surface_from(
        &mut self,
        width: u32,
        height: u32,
        src_texture: &ID3D11Texture2D,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        match *self.texture.texture_for(width, height) {
            Some((ref surface, ref mutex)) => {
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    copy_to_surface(&self.cx, width, height, surface, src_texture, rect)?;
                }

                Ok(None)
            }

            ref mut slot @ None => {
                let (surface, mutex) =
                    slot.insert(create_surface_texture(&self.device, width, height, None)?);
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    copy_to_surface(&self.cx, width, height, surface, src_texture, rect)?;
                }

                Ok(Some(UpdateSharedHandle {
                    handle: NonZeroU32::new(
                        unsafe { surface.cast::<IDXGIResource>()?.GetSharedHandle() }?.0 as u32,
                    ),
                }))
            }
        }
    }

    /// Update the surface from a bitmap data.
    /// The bitmap data should be in BGRA format.
    /// * Returns [`None`]` if the update is done to an existing internal texture.
    /// * Returns [`Some`]` if a new internal texture is created, due to size change.
    /// * Returns error if failed to create or update the internal texture.
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
                        handle: NonZeroU32::new(
                            texture.cast::<IDXGIResource>()?.GetSharedHandle()?.0 as u32,
                        ),
                    }))
                }
            }
        }
    }
}

/// Copy a region from one texture to another.
fn copy_to_surface(
    cx: &ID3D11DeviceContext,
    width: u32,
    height: u32,
    surface: &ID3D11Texture2D,
    src: &ID3D11Texture2D,
    rect: Option<CopyRect>,
) -> anyhow::Result<()> {
    #[inline]
    fn is_out(x: u32, y: u32, width: u32, height: u32) -> bool {
        x > width || y > height
    }

    let mut src_desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        src.GetDesc(&mut src_desc);
    }

    match rect {
        Some(rect) => unsafe {
            if is_out(rect.dst_x, rect.dst_y, width, height)
                || is_out(
                    rect.dst_x + rect.src.width,
                    rect.dst_y + rect.src.height,
                    width,
                    height,
                )
                || is_out(rect.src.x, rect.src.y, src_desc.Width, src_desc.Height)
                || is_out(
                    rect.src.x + rect.src.width,
                    rect.src.y + rect.src.height,
                    src_desc.Width,
                    src_desc.Height,
                )
            {
                bail!("CopyRect is out of range");
            }

            cx.CopySubresourceRegion(
                surface,
                0,
                rect.dst_x,
                rect.dst_y,
                0,
                src,
                0,
                Some(&D3D11_BOX {
                    left: rect.src.x,
                    top: rect.src.y,
                    front: 0,
                    right: rect.src.x + rect.src.width,
                    bottom: rect.src.y + rect.src.height,
                    back: 1,
                }),
            );
        },

        _ => unsafe {
            cx.CopyResource(surface, src);
        },
    }

    Ok(())
}

/// Create a Direct3D texture and returns texture with its keyed mutex.
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

/// A simple ring buffer for Direct3D textures.
struct BufferedTexture<const BUFFERS: usize> {
    texture: [Option<(ID3D11Texture2D, IDXGIKeyedMutex)>; BUFFERS],
    index: usize,
}

impl<const BUFFERS: usize> BufferedTexture<BUFFERS> {
    /// Create a new [`BufferedTexture`].
    pub fn new() -> Self {
        Self {
            texture: [const { None }; BUFFERS],
            index: 0,
        }
    }

    /// Get a mutable reference to the texture slot for the given size.
     /// This will rotate the buffer if the size is different from the current texture.
     /// The returned slot is [`None`] if a new texture needs to be created.
     /// The returned slot is [`Some`] if the texture can be reused.
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
