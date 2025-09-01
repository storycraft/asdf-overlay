//! Provides abstraction for overlay surface.
//!
//! The surface texture must be Direct3D 11 texture created with shared flags.
//! Direct3D 11 was chosen, because it is well supported on almost every gpus nowadays.
//!
//! If you create surface texture with keyed mutex, it will uses it for synchronization.
//! You must keep mutex key to `0` otherwise, it will wait indefinitely when rendering overlay.
//! You can still have surface texture without keyed mutex,
//! however you must flush it manually on changes and will have worse performance.

use core::num::NonZeroU32;

use anyhow::Context;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::{
            Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D},
            Dxgi::{IDXGIKeyedMutex, IDXGIResource},
        },
    },
    core::Interface,
};

/// The overlay surface texture.
pub struct OverlaySurface {
    texture: ID3D11Texture2D,
    resource: IDXGIResource,
    mutex: Option<IDXGIKeyedMutex>,
    size: (u32, u32),
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
    /// [`ID3D11Texture2D`] of the surface texture.
    pub const fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    #[inline]
    /// Shared handle of the surface texture.
    pub fn shared_handle(&self) -> NonZeroU32 {
        NonZeroU32::new(unsafe { self.resource.GetSharedHandle().unwrap().0 as _ }).unwrap()
    }
}
