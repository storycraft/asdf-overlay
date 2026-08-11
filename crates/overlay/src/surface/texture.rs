//! Overlay surface abstraction.
//!
//! The surface texture must be Direct3D 11 texture created with shared flags.
//! Direct3D 11 was chosen, because it is well supported on almost every gpus nowadays.
//!
//! If you create surface texture with keyed mutex, it will uses it for synchronization.
//! You must keep mutex key to `0` otherwise, it will wait indefinitely when rendering overlay.
//! You can still have surface texture without keyed mutex,
//! however you must flush it manually on changes and will have worse performance.

use core::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use parking_lot::{RwLock, RwLockReadGuard};
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::{
            Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D},
            Dxgi::{Common::DXGI_FORMAT, IDXGIKeyedMutex, IDXGIResource},
        },
    },
    core::Interface,
};

/// Overlay surface texture.
pub struct OverlaySurface {
    texture: ID3D11Texture2D,
    resource: IDXGIResource,
    mutex: Option<IDXGIKeyedMutex>,
    size: (u32, u32),
    format: DXGI_FORMAT,
}

impl OverlaySurface {
    /// Open Direct3D 11 shared texture using `handle`, with given `device`.
    pub(crate) fn open_shared(device: &ID3D11Device, handle: u32) -> anyhow::Result<Self> {
        unsafe {
            let mut texture = None::<ID3D11Texture2D>;
            device
                .OpenSharedResource(HANDLE(handle as _), &mut texture)
                .context("failed to open shared resource")?;
            let texture = texture.unwrap();

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);

            let resource = texture.cast::<IDXGIResource>().unwrap();
            let mutex = texture.cast::<IDXGIKeyedMutex>().ok();
            Ok(Self {
                texture,
                resource,
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
    /// Shared handle of the surface texture.
    pub fn shared_handle(&self) -> u32 {
        unsafe { self.resource.GetSharedHandle().unwrap().0 as _ }
    }
}

pub struct OverlayTextureSlot {
    inner: RwLock<Option<OverlaySurface>>,
    updated: AtomicBool,
}

impl OverlayTextureSlot {
    pub(crate) const fn new() -> Self {
        Self {
            inner: RwLock::new(None),
            updated: AtomicBool::new(true),
        }
    }

    #[doc(hidden)]
    pub fn get(&self) -> RwLockReadGuard<'_, Option<OverlaySurface>> {
        self.inner.read()
    }

    pub(super) fn update(&self, device: &ID3D11Device, handle: Option<u32>) -> anyhow::Result<()> {
        self.updated.store(true, Ordering::Relaxed);
        let Some(handle) = handle else {
            *self.inner.write() = None;
            return Ok(());
        };

        *self.inner.write() = Some(OverlaySurface::open_shared(device, handle)?);
        Ok(())
    }

    #[inline]
    #[doc(hidden)]
    pub fn take_update(&self) -> Option<Option<u32>> {
        if !self.invalidate_update() {
            return None;
        }

        Some(
            self.inner
                .read()
                .as_ref()
                .map(|surface| surface.shared_handle()),
        )
    }

    #[inline]
    #[doc(hidden)]
    pub fn invalidate_update(&self) -> bool {
        self.updated.swap(false, Ordering::Relaxed)
    }
}
