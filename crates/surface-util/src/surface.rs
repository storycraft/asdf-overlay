//! Client side overlay surface management wrapper.
//!
//! Uses Direct3D11 to manage overlay surfaces
//! and provide convenient methods to update them from bitmaps or other shared texture.

use core::ptr;

use anyhow::{Context, bail};
use asdf_overlay_common::request::surface::UpdateSharedHandle;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HANDLE, HMODULE},
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
                },
                IDXGIAdapter, IDXGIKeyedMutex, IDXGIResource,
            },
        },
    },
    core::Interface,
};

use crate::ty::CopyRect;

/// Represents an overlay surface.
///
/// Retains a ring of textures, rotating only when dimensions or format change;
/// same-size, same-format updates reuse the current texture. `BUFFERS` must be
/// greater than zero or a nonempty update will panic.
///
/// Update methods return `Some(handle)` when the consumer must change its shared
/// handle, `None` when the current handle remains usable, and
/// `Some(UpdateSharedHandle::None)` to request removal. Forward only `Some`
/// updates to the consumer. Empty updates request removal without freeing the
/// cached textures; use [`Self::clear`] to release them.
///
/// Shared resources must use the consumer's GPU adapter. Keyed mutexes use key
/// zero and infinite waits, so an unreleased mutex can block an update indefinitely.
pub struct OverlaySurface<const BUFFERS: usize = 2> {
    device: ID3D11Device,
    cx: ID3D11DeviceContext,

    texture: BufferedTexture<BUFFERS>,
}

impl<const BUFFERS: usize> OverlaySurface<BUFFERS> {
    /// Create a new [`OverlaySurface`].
    /// This will create a Direct3D11 device and context internally.
    /// * Returns error if failed to create Direct3D11 device or context.
    pub fn new(adapter: Option<&IDXGIAdapter>) -> anyhow::Result<Self> {
        let mut device = None;
        let mut cx = None;
        unsafe {
            D3D11CreateDevice(
                adapter,
                if adapter.is_none() {
                    D3D_DRIVER_TYPE_HARDWARE
                } else {
                    D3D_DRIVER_TYPE_UNKNOWN
                },
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut cx),
            )?;
        }
        let device = device.context("failed to create Dx11 Device")?;
        let cx = cx.context("failed to create Dx11 Context")?;

        Ok(Self::new_with_device(device, cx))
    }

    /// Wrap an existing device and its immediate context without allocating textures.
    ///
    /// The context must belong to this device. The pair is not validated, and
    /// callers must synchronize any other use of the same immediate context.
    /// The device must support shared shader-resource textures on the target GPU.
    pub fn new_with_device(device: ID3D11Device, cx: ID3D11DeviceContext) -> Self {
        Self {
            device,
            cx,
            texture: BufferedTexture::new(),
        }
    }

    /// Release cached textures so the next nonempty update creates a new handle.
    ///
    /// This does not notify the consumer or clear its already-open texture. Send
    /// `UpdateSharedHandle::None` separately when the overlay should disappear.
    pub fn clear(&mut self) {
        self.texture = BufferedTexture::new();
    }

    /// Open a D3D11 NT shared texture and copy it using [`Self::update_from_texture`].
    ///
    /// The handle must be valid in this process; a handle from another process
    /// must first be duplicated here. KMT handles are not interchangeable with NT
    /// handles. This borrows the handle and does not close it.
    ///
    /// Opening happens even for zero dimensions and can fail. If present, the
    /// source keyed mutex is acquired at key zero with an infinite wait; its
    /// acquire/release errors are currently ignored. See the texture-copy method
    /// for rectangle restrictions and the three possible successful return values.
    pub fn update_from_nt_shared(
        &mut self,
        width: u32,
        height: u32,
        handle: u32,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let device1 = self.device.cast::<ID3D11Device1>()?;
        let src_texture =
            unsafe { device1.OpenSharedResource1::<ID3D11Texture2D>(HANDLE(handle as _)) }
                .context("opening NT shared texture")?;
        with_external_texture(&src_texture, |src_texture| {
            self.update_from_texture(width, height, src_texture, rect)
        })
    }

    /// Open a D3D11 KMT shared texture and copy it using [`Self::update_from_texture`].
    ///
    /// Supply a legacy shared-resource handle, not an NT handle, and keep its
    /// source resource alive. The resource must be accessible on this device's GPU.
    /// Opening happens even for zero dimensions and can return an error.
    ///
    /// If present, the source keyed mutex is acquired at key zero with an infinite
    /// wait; its acquire/release errors are currently ignored. See the texture-copy
    /// method for rectangle restrictions and successful return values.
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
                .OpenSharedResource::<ID3D11Texture2D>(HANDLE(handle as _), &mut src_texture)
                .context("opening KMT shared texture")?
        };
        with_external_texture(&src_texture.unwrap(), |src_texture| {
            self.update_from_texture(width, height, src_texture, rect)
        })
    }

    /// Copy a source texture into a shared texture of the requested dimensions.
    ///
    /// The source must be usable by this device. With no rectangle, this performs
    /// a whole-resource copy: dimensions and resource layout must match; it does
    /// not scale, resolve multisampling, or convert formats. With a rectangle,
    /// source and destination bounds must fit, and coordinate-plus-extent sums
    /// must not overflow `u32`. Out-of-bounds rectangles return an error.
    ///
    /// The source mutex is not acquired here; synchronize source access yourself.
    /// Destination mutex acquisition can wait indefinitely. D3D copy commands do
    /// not return validation errors, so `Ok` is not proof that an incompatible
    /// copy succeeded. A partial copy into a newly allocated texture leaves pixels
    /// outside the copied region uninitialized.
    ///
    /// Returns `None` when reusing the current texture and `Some(Kmt(_))` when
    /// dimensions or format require a new texture. Either zero dimension returns
    /// `Some(UpdateSharedHandle::None)` without copying or clearing cached textures.
    /// Texture creation, sharing, and destination mutex failures return errors.
    pub fn update_from_texture(
        &mut self,
        width: u32,
        height: u32,
        src_texture: &ID3D11Texture2D,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        if width == 0 || height == 0 {
            return Ok(Some(UpdateSharedHandle::None));
        }

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            src_texture.GetDesc(&mut desc);
        }

        let format = desc.Format;
        match *self.texture.texture_for(width, height, format) {
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
                    create_surface_texture(&self.device, width, height, format, None)?;
                unsafe {
                    mutex.AcquireSync(0, u32::MAX)?;
                    defer!({
                        _ = mutex.ReleaseSync(0);
                    });

                    copy_to_surface(&self.cx, width, height, &surface, src_texture, rect)?;
                }

                let update = UpdateSharedHandle::Kmt(
                    unsafe { surface.cast::<IDXGIResource>()?.GetSharedHandle() }?.0 as _,
                );
                *slot = Some((surface, mutex));
                Ok(Some(update))
            }
        }
    }

    /// Upload tightly packed, four-byte BGRA pixels without row padding.
    ///
    /// Height is `data.len() / width / 4`, rounded down: incomplete trailing rows
    /// are ignored, not rejected. A nonempty buffer shorter than one row yields
    /// zero height and texture creation fails. RGBA bytes are accepted but their
    /// red and blue channels are interpreted as BGRA. Use dimensions supported by
    /// D3D11, with `width * 4` and the computed height representable as `u32`.
    ///
    /// Zero width or empty data returns `Some(UpdateSharedHandle::None)` without
    /// releasing cached textures. Otherwise returns `None` for texture reuse or
    /// `Some(Kmt(_))` for a new texture, including after a format change.
    /// Allocation, sharing, and mutex failures return errors. Updating an existing
    /// texture can wait indefinitely for its key-zero mutex; the upload itself
    /// has no D3D error return and does not wait for presentation.
    pub fn update_bitmap(
        &mut self,
        width: u32,
        data: &[u8],
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        if width == 0 || data.is_empty() {
            return Ok(Some(UpdateSharedHandle::None));
        }

        let size = (width, (data.len() / width as usize / 4) as u32);
        let surface = self
            .texture
            .texture_for(size.0, size.1, DXGI_FORMAT_B8G8R8A8_UNORM);

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
                    DXGI_FORMAT_B8G8R8A8_UNORM,
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

                    Ok(Some(UpdateSharedHandle::Kmt(
                        texture.cast::<IDXGIResource>()?.GetSharedHandle()?.0 as _,
                    )))
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

/// Perform an operation with an external texture, acquiring and releasing its keyed mutex if available.
fn with_external_texture<R>(texture: &ID3D11Texture2D, f: impl FnOnce(&ID3D11Texture2D) -> R) -> R {
    if let Ok(mutex) = texture.cast::<IDXGIKeyedMutex>() {
        unsafe {
            _ = mutex.AcquireSync(0, u32::MAX);
        }
        defer!(unsafe {
            _ = mutex.ReleaseSync(0);
        });

        f(texture)
    } else {
        f(texture)
    }
}

/// Create a Direct3D texture and returns texture with its keyed mutex.
fn create_surface_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    initial: Option<&D3D11_SUBRESOURCE_DATA>,
) -> anyhow::Result<(ID3D11Texture2D, IDXGIKeyedMutex)> {
    let mut texture = None;
    unsafe {
        device
            .CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: format,
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
            )
            .context("cannot create buffer texture")?;
        let texture = texture.unwrap();
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
    /// * The returned slot is [`None`] if a new texture needs to be created.
    /// * The returned slot is [`Some`] if the texture can be reused.
    pub fn texture_for(
        &mut self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> &mut Option<(ID3D11Texture2D, IDXGIKeyedMutex)> {
        let prev = if let Some((ref texture, _)) = self.texture[self.index] {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                texture.GetDesc(&mut desc);
            }

            (desc.Width, desc.Height, desc.Format)
        } else {
            (0, 0, DXGI_FORMAT_UNKNOWN)
        };

        if prev.0 != width || prev.1 != height || prev.2 != format {
            self.index = (self.index + 1) % BUFFERS;
            let texture = &mut self.texture[self.index];
            texture.take();

            texture
        } else {
            &mut self.texture[self.index]
        }
    }
}
