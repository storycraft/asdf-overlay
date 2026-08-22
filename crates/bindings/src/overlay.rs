use anyhow::Context;
use asdf_overlay::{
    event_sink::OverlayEventSink,
    surface::{self, Surfaces},
};
use flume::Receiver;

use crate::{
    event::surface::{OverlayEvent, SurfaceId, SurfaceInfo},
    types,
};

extern crate asdf_overlay_vulkan_layer;
#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Overlay {
    rx: Receiver<OverlayEvent>,
}

#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Overlay {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    #[cfg_attr(feature = "napi", napi(factory))]
    pub fn initialize() -> types::Result<Self> {
        asdf_overlay::initialize()?;

        let (tx, rx) = flume::unbounded();

        OverlayEventSink::set(move |event| {
            _ = tx.send(OverlayEvent::from(event));
        });
        Ok(Self { rx })
    }

    #[cfg_attr(feature = "napi", napi)]
    pub fn recv_event(&self) -> Option<OverlayEvent> {
        self.rx.recv().ok()
    }

    #[cfg_attr(feature = "napi", napi)]
    pub fn surface_info(&self, id: SurfaceId) -> Option<SurfaceInfo> {
        Surfaces::state(id.0, |state| SurfaceInfo::from(state.info))
    }

    #[cfg_attr(feature = "napi", napi)]
    pub fn surfaces(&self) -> Vec<SurfaceId> {
        Surfaces::iter().map(SurfaceId).collect()
    }

    #[cfg_attr(feature = "napi", napi)]
    pub fn reposition_surface(&self, id: SurfaceId, x: i32, y: i32) -> bool {
        Surfaces::state(id.0, |state| state.reposition(x, y)).is_some()
    }

    #[cfg_attr(feature = "napi", napi)]
    pub fn commit_overlay_surface(
        &self,
        id: SurfaceId,
        handle: Option<SharedTextureHandle>,
    ) -> types::Result<()> {
        Ok(Surfaces::state(id.0, |state| {
            state.commit_overlay_texture(handle.map(Into::into))
        })
        .context("surface is not found")
        .flatten()?)
    }
}

#[cfg(feature = "async")]
#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Overlay {
    #[cfg_attr(feature = "napi", napi)]
    pub async fn recv_event_async(&self) -> Option<OverlayEvent> {
        self.rx.recv_async().await.ok()
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        OverlayEventSink::clear();
    }
}

#[cfg_attr(feature = "napi", napi_derive::napi)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SharedTextureHandle {
    Kmt { handle: u32 },
    Nt { handle: u32 },
}

impl From<surface::SharedTextureHandle> for SharedTextureHandle {
    fn from(update: surface::SharedTextureHandle) -> Self {
        match update {
            surface::SharedTextureHandle::Kmt(handle) => Self::Kmt { handle },
            surface::SharedTextureHandle::Nt(handle) => Self::Nt { handle },
        }
    }
}

impl From<SharedTextureHandle> for surface::SharedTextureHandle {
    fn from(val: SharedTextureHandle) -> Self {
        match val {
            SharedTextureHandle::Kmt { handle } => Self::Kmt(handle),
            SharedTextureHandle::Nt { handle } => Self::Nt(handle),
        }
    }
}
