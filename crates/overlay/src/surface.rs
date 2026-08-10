//! Manage window states for rendering overlays.
//! You can access states for specific window using [`Backends::with_backend`].
//! This allows you to interact with the overlay state of a window, including its layout and rendering data.

pub mod texture;

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use anyhow::Context;
use asdf_overlay_event::{Event, SurfaceEvent};
use once_cell::sync::Lazy;
use windows::Win32::Graphics::Dxgi::IDXGIAdapter;

use crate::{
    event_sink::OverlayEventSink, interop::DxInterop, surface::texture::SurfaceTextureSlot,
    types::IntDashMap,
};

static SURFACES: Lazy<Surfaces> = Lazy::new(|| Surfaces {
    map: IntDashMap::default(),
});

/// Global store for surface states.
pub struct Surfaces {
    map: IntDashMap<u64, SurfaceState>,
}

impl Surfaces {
    /// Iterate over all surfaces.
    pub fn iter<'a>() -> impl Iterator<Item = u64> {
        SURFACES.map.iter().map(|r| *r.key())
    }

    /// Run closure with the specified surface, if it exists.
    pub fn state<R>(id: u64, f: impl FnOnce(&SurfaceState) -> R) -> Option<R> {
        Some(f(&*SURFACES.map.get(&id)?))
    }

    pub fn reset() {
        for state in SURFACES.map.iter() {
            state.reset();
        }
    }

    #[doc(hidden)]
    pub fn with<R>(
        id: u64,
        setup_fn: impl FnOnce() -> anyhow::Result<SurfaceState>,
        f: impl FnOnce(&SurfaceState) -> R,
    ) -> anyhow::Result<R> {
        if let Some(backend) = SURFACES.map.get(&id) {
            return Ok(f(&backend));
        }

        let backend = SURFACES
            .map
            .entry(id)
            .or_try_insert_with(|| {
                let state = setup_fn().context("failed to setup surface state")?;

                let (width, height) = state.size();
                OverlayEventSink::emit(Event::Surface {
                    id,
                    event: SurfaceEvent::Added {
                        width,
                        height,
                        gpu_id: state.interop.gpu_id(),
                    },
                });

                Ok::<_, anyhow::Error>(state)
            })?
            .downgrade();

        Ok(f(backend.value()))
    }

    #[doc(hidden)]
    pub fn cleanup_state(id: u64) {
        SURFACES.map.remove(&id);

        OverlayEventSink::emit(Event::Surface {
            id,
            event: SurfaceEvent::Destroyed,
        });
    }
}

/// Data associated to a specific window for overlay rendering.
#[non_exhaustive]
pub struct SurfaceState {
    position: (AtomicI32, AtomicI32),
    pub size: (AtomicU32, AtomicU32),

    pub interop: DxInterop,
    pub renderer: Renderer,

    #[doc(hidden)]
    pub surface: SurfaceTextureSlot,
}

impl SurfaceState {
    pub fn new(
        adapter: Option<&IDXGIAdapter>,
        size: (u32, u32),
        renderer: Renderer,
    ) -> anyhow::Result<Self> {
        let interop = DxInterop::create(adapter).context("failed to create dx interop")?;
        let surface = SurfaceTextureSlot::new();

        Ok(Self {
            position: (AtomicI32::new(0), AtomicI32::new(0)),
            size: (AtomicU32::new(size.0), AtomicU32::new(size.1)),
            interop,
            renderer,
            surface,
        })
    }

    #[doc(hidden)]
    pub fn surface_size(&self) -> Option<(u32, u32)> {
        self.surface.get().as_ref().map(|surface| surface.size())
    }

    pub fn size(&self) -> (u32, u32) {
        (
            self.size.0.load(Ordering::Relaxed),
            self.size.1.load(Ordering::Relaxed),
        )
    }

    pub fn position(&self) -> (i32, i32) {
        (
            self.position.0.load(Ordering::Relaxed),
            self.position.1.load(Ordering::Relaxed),
        )
    }

    pub fn set_position(&self, x: i32, y: i32) {
        self.position.0.store(x, Ordering::Relaxed);
        self.position.1.store(y, Ordering::Relaxed);
    }

    pub fn set_overlay_texture(&self, handle: Option<u32>) -> anyhow::Result<()> {
        self.surface.update(&self.interop.device, handle)
    }

    /// Reset the surface state to its initial state.
    /// This will reset the position to (0, 0) and remove the overlay texture
    pub fn reset(&self) {
        self.set_position(0, 0);
        _ = self.set_overlay_texture(None);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Renderer {
    Dx12,
    Dx11,
    Dx9,
    Opengl,
    Vulkan,
}
