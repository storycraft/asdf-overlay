//! Shared D3D11 textures for overlay rendering.
//!
//! Release keyed mutexes at key zero before rendering. Without a keyed mutex,
//! flush texture changes manually.

use core::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use parking_lot::{RwLock, RwLockReadGuard};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::{
            Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Device1, ID3D11Texture2D},
            Dxgi::{Common::DXGI_FORMAT, IDXGIKeyedMutex},
        },
    },
    core::Interface,
};

use crate::surface::SharedTextureHandle;

/// Overlay surface texture.
pub struct OverlaySurface {
    texture: ID3D11Texture2D,
    handle: SharedTextureHandle,
    mutex: Option<IDXGIKeyedMutex>,
    size: (u32, u32),
    format: DXGI_FORMAT,
}

impl OverlaySurface {
    /// Open Direct3D 11 shared texture by consuming `handle`, with given `device`.
    pub(crate) fn open(device: &ID3D11Device, handle: SharedTextureHandle) -> anyhow::Result<Self> {
        unsafe {
            let texture = match handle {
                SharedTextureHandle::Kmt(handle) => {
                    let mut slot = None::<ID3D11Texture2D>;
                    device
                        .OpenSharedResource(HANDLE(handle as _), &mut slot)
                        .context("failed to open KMT shared texture")?;

                    slot.unwrap()
                }

                SharedTextureHandle::Nt(handle) => device
                    .cast::<ID3D11Device1>()?
                    .OpenSharedResource1::<ID3D11Texture2D>(HANDLE(handle as _))
                    .context("failed to open NT shared texture")?,
            };

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);

            let mutex = texture.cast::<IDXGIKeyedMutex>().ok();
            Ok(Self {
                texture,
                handle,
                mutex,
                size: (desc.Width, desc.Height),
                format: desc.Format,
            })
        }
    }

    #[inline]
    /// [`IDXGIKeyedMutex`] of the surface texture.
    pub const fn mutex(&self) -> Option<&IDXGIKeyedMutex> {
        self.mutex.as_ref()
    }

    #[inline]
    /// Size of the overlay surface in phyiscal pixel units.
    pub const fn size(&self) -> (u32, u32) {
        self.size
    }

    #[inline]
    /// Format of the overlay surface.
    pub const fn format(&self) -> DXGI_FORMAT {
        self.format
    }

    #[inline]
    /// [`ID3D11Texture2D`] of the surface texture.
    pub const fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    #[inline]
    /// Borrow the handle value. The texture retains ownership; do not close or
    /// transfer the returned NT handle.
    pub fn shared_handle(&self) -> SharedTextureHandle {
        self.handle
    }
}

impl Drop for OverlaySurface {
    fn drop(&mut self) {
        if let SharedTextureHandle::Nt(handle) = self.handle {
            unsafe {
                _ = CloseHandle(HANDLE(handle as _));
            }
        }
    }
}

pub struct OverlayTextureSlot {
    inner: RwLock<Option<OverlaySurface>>,
    generation: AtomicU64,
}

impl OverlayTextureSlot {
    pub(crate) const fn new() -> Self {
        Self {
            inner: RwLock::new(None),
            generation: AtomicU64::new(1),
        }
    }

    #[doc(hidden)]
    pub fn get(&self) -> RwLockReadGuard<'_, Option<OverlaySurface>> {
        self.inner.read()
    }

    /// Mark the current texture as changed, so every renderer uploads it again.
    #[inline]
    /// Mark the slot changed for renderers without modifying texture contents.
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Current texture generation, bumped on every change.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(super) fn update(
        &self,
        device: &ID3D11Device,
        handle: Option<SharedTextureHandle>,
    ) -> anyhow::Result<()> {
        let Some(handle) = handle else {
            *self.inner.write() = None;
            self.generation.fetch_add(1, Ordering::Release);
            return Ok(());
        };

        let surface = OverlaySurface::open(device, handle)?;
        *self.inner.write() = Some(surface);
        // Bump after the texture is in place, so a renderer that observes the new
        // generation is guaranteed to read the texture that goes with it.
        self.generation.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

/// Tracks the texture generation a single renderer has uploaded.
///
/// A renderer cannot re-read the texture it already holds, so the slot cannot simply
/// clear a shared "updated" flag: the first renderer to draw would consume the update
/// and every other renderer of the same surface would miss it. Keeping the last
/// uploaded generation per renderer also means a renderer recreated from scratch
/// starts at zero and uploads again, instead of drawing nothing until the client
/// happens to commit a new texture.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextureGeneration(u64);

impl TextureGeneration {
    #[inline]
    pub const fn new() -> Self {
        Self(0)
    }

    /// Return whether `slot` holds a texture this renderer has not uploaded yet,
    /// recording it as uploaded.
    ///
    /// Unlike a shared flag, every renderer of the same surface observes the change.
    /// This does not indicate GPU completion.
    #[inline]
    pub fn take_update(&mut self, slot: &OverlayTextureSlot) -> bool {
        let generation = slot.generation();
        if self.0 == generation {
            return false;
        }

        self.0 = generation;
        true
    }
}
