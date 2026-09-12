//! Manage rendering surface states through [`Surfaces::state`].
//!
//! Surface IDs identify graphics surfaces, not windows. Several surfaces may belong
//! to one window, and composition surfaces may have no window at all.

pub mod texture;

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use anyhow::Context;
use asdf_overlay_event::{Event, SurfaceEvent, SurfaceInfo};
use once_cell::sync::Lazy;

use crate::{
    event_sink::OverlayEventSink, interop::DxInterop, surface::texture::OverlayTextureSlot,
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
    /// Iterate over currently tracked surface IDs in unspecified order.
    ///
    /// This is a live map iterator, not a snapshot; a yielded ID may disappear
    /// before a later lookup. Avoid mutating the registry while holding an iterator.
    pub fn iter() -> impl Iterator<Item = u64> {
        SURFACES.map.iter().map(|r| *r.key())
    }

    /// Run the closure synchronously under the surface map's read guard.
    ///
    /// Returns `Some` of the closure's result, or `None` without calling it if
    /// the ID is unknown or destroyed. Do not perform operations that need a
    /// write lock on the registry from the closure; they can deadlock.
    pub fn state<R>(id: u64, f: impl FnOnce(&SurfaceState) -> R) -> Option<R> {
        SURFACES.map.get(&id).map(|r| f(&r))
    }

    /// Check whether an ID is tracked now; this does not reserve it for later use.
    pub fn contains(id: u64) -> bool {
        SURFACES.map.contains_key(&id)
    }

    /// Reset every tracked position and texture without removing surface entries.
    ///
    /// This does not uninstall hooks or emit surface-destruction events.
    pub fn reset() {
        for state in SURFACES.map.iter() {
            state.reset();
        }
    }

    #[doc(hidden)]
    pub fn with<R>(
        id: u64,
        setup_fn: impl FnOnce() -> anyhow::Result<SurfaceState>,
        f: impl FnOnce(&SurfaceState) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        if let Some(backend) = SURFACES.map.get(&id) {
            return f(&backend);
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
                        info: state.info,
                    },
                });

                Ok::<_, anyhow::Error>(state)
            })?
            .downgrade();

        f(backend.value())
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
    size: (AtomicU32, AtomicU32),

    pub interop: DxInterop,
    pub info: SurfaceInfo,

    #[doc(hidden)]
    pub texture: OverlayTextureSlot,
}

impl SurfaceState {
    /// Construct an unregistered state at position `(0, 0)` with no overlay texture.
    ///
    /// Size and metadata are stored without validation; this does not allocate a
    /// texture or emit an event. Currently always returns `Ok`.
    pub fn new(interop: DxInterop, size: (u32, u32), info: SurfaceInfo) -> anyhow::Result<Self> {
        let surface = OverlayTextureSlot::new();

        Ok(Self {
            position: (AtomicI32::new(0), AtomicI32::new(0)),
            size: (AtomicU32::new(size.0), AtomicU32::new(size.1)),
            interop,
            info,
            texture: surface,
        })
    }

    #[doc(hidden)]
    pub fn texture_size(&self) -> Option<(u32, u32)> {
        self.texture.get().as_ref().map(|surface| surface.size())
    }

    /// Return the tracked graphics-surface dimensions in physical pixels.
    ///
    /// These are not overlay-texture dimensions. Width and height are read
    /// separately, so a concurrent resize can produce a mixed pair.
    pub fn size(&self) -> (u32, u32) {
        (
            self.size.0.load(Ordering::Relaxed),
            self.size.1.load(Ordering::Relaxed),
        )
    }

    #[doc(hidden)]
    pub fn resize(&self, width: u32, height: u32) {
        self.size.0.store(width, Ordering::Relaxed);
        self.size.1.store(height, Ordering::Relaxed);
    }

    /// Return the overlay offset in physical pixels relative to the target surface.
    ///
    /// Coordinates are read separately and need not be a snapshot of a concurrent move.
    pub fn position(&self) -> (i32, i32) {
        (
            self.position.0.load(Ordering::Relaxed),
            self.position.1.load(Ordering::Relaxed),
        )
    }

    /// Set the overlay offset for subsequent rendering without moving the host window.
    ///
    /// Negative and off-surface coordinates are accepted without clamping. The
    /// two coordinates are stored separately; this does not synchronize with a frame.
    pub fn reposition(&self, x: i32, y: i32) {
        self.position.0.store(x, Ordering::Relaxed);
        self.position.1.store(y, Ordering::Relaxed);
    }

    /// Open and replace the shared overlay texture, or remove it with `None`.
    ///
    /// Supply a D3D11 shared texture compatible with this state's GPU. An NT
    /// handle must already be valid in this process; it is not duplicated here.
    /// On success the texture owns that NT handle and closes it when dropped.
    /// Do not close it or transfer the same handle to another owner. On opening
    /// failure the old texture remains and the supplied NT handle is not closed.
    ///
    /// KMT handles require their source resource to remain alive. If a keyed mutex
    /// is present, producers must release key zero for rendering. Opening errors
    /// are returned; success does not wait for a frame to display the texture.
    pub fn commit_overlay_texture(
        &self,
        handle: Option<SharedTextureHandle>,
    ) -> anyhow::Result<()> {
        self.texture.update(&self.interop.device, handle)
    }

    /// Reset the surface state to its initial state.
    /// This will reset the position to (0, 0) and remove the overlay texture
    pub fn reset(&self) {
        self.reposition(0, 0);
        _ = self.commit_overlay_texture(None);
    }
}

/// A shared D3D11 texture handle with its required opening mechanism.
///
/// The enum itself does not close handles. Although it is `Copy`, copying an NT
/// value does not duplicate the OS handle or create another ownership right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedTextureHandle {
    /// KMT handle.
    Kmt(u32),

    /// Owned NT handle.
    Nt(u32),
}

impl SharedTextureHandle {
    /// Return the stored bits without duplicating, validating, or transferring ownership.
    ///
    /// The number alone does not distinguish NT from KMT handles.
    pub fn as_raw(&self) -> u32 {
        match self {
            Self::Kmt(handle) | Self::Nt(handle) => *handle,
        }
    }
}
