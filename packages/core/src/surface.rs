use anyhow::Context;
use asdf_overlay_client::common::request;
use asdf_overlay_surface_util::{surface, ty};
use bytemuck::try_pod_read_unaligned;
use napi::bindgen_prelude::BufferSlice;
use napi_derive::napi;
use windows::Win32::{
    Foundation::LUID,
    Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
};

use crate::event::surface::GpuLuid;

/// A shared overlay texture backed by [`surface::OverlaySurface`].
///
/// Update methods return `None` when the consumer can reuse its handle. Forward
/// `Some` updates to the consumer to replace or remove the texture.
///
/// Shared resources must use the consumer's GPU. Handle buffers contain one
/// native-endian, pointer-sized integer whose low 32 bits identify the handle.
/// See [`surface::OverlaySurface`] for synchronization requirements.
#[napi]
pub struct OverlaySurface(surface::OverlaySurface);

#[napi]
impl OverlaySurface {
    /// Create a surface on the adapter matching the LUID.
    ///
    /// Uses the default hardware GPU if the LUID is omitted or not found.
    #[napi(constructor)]
    pub fn new(luid: Option<GpuLuid>) -> anyhow::Result<Self> {
        let adapter = luid.map(create_adapter_by_luid).transpose()?.flatten();
        let surface = surface::OverlaySurface::new(adapter.as_ref())?;
        Ok(Self(surface))
    }

    /// Release cached textures. Send a removal update separately to hide the overlay.
    #[napi]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Copy a texture from a borrowed NT handle valid in this process.
    ///
    /// See [`surface::OverlaySurface::update_from_nt_shared`] for copy requirements.
    #[napi]
    pub fn update_nt_shtex(
        &mut self,
        width: u32,
        height: u32,
        handle: BufferSlice,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let handle =
            try_pod_read_unaligned::<usize>(&handle).context("invalid surface handle size")?;
        Ok(self
            .0
            .update_from_nt_shared(width, height, handle as _, rect.map(Into::into))?
            .map(From::from))
    }

    /// Copy a texture from a legacy KMT shared handle.
    ///
    /// See [`surface::OverlaySurface::update_from_shared`] for copy requirements.
    #[napi]
    pub fn update_kmt_shtex(
        &mut self,
        width: u32,
        height: u32,
        handle: BufferSlice,
        rect: Option<CopyRect>,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        let handle =
            try_pod_read_unaligned::<usize>(&handle).context("invalid surface handle size")?;
        Ok(self
            .0
            .update_from_shared(width, height, handle as _, rect.map(Into::into))?
            .map(From::from))
    }

    /// Upload tightly packed BGRA pixels.
    ///
    /// Height is derived from complete rows; trailing partial rows are ignored.
    /// Zero width or empty data requests removal. See
    /// [`surface::OverlaySurface::update_bitmap`] for buffer requirements.
    #[napi]
    pub fn update_bitmap(
        &mut self,
        width: u32,
        data: BufferSlice,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        Ok(self.0.update_bitmap(width, &data)?.map(From::from))
    }
}

#[napi]
pub enum UpdateSharedHandle {
    Kmt(u32),
    Nt(u32),
    None,
}

impl From<request::surface::UpdateSharedHandle> for UpdateSharedHandle {
    fn from(update: request::surface::UpdateSharedHandle) -> Self {
        match update {
            request::surface::UpdateSharedHandle::Kmt(handle) => Self::Kmt(handle),
            request::surface::UpdateSharedHandle::Nt(handle) => Self::Nt(handle),
            request::surface::UpdateSharedHandle::None => Self::None,
        }
    }
}

impl From<UpdateSharedHandle> for request::surface::UpdateSharedHandle {
    fn from(val: UpdateSharedHandle) -> Self {
        match val {
            UpdateSharedHandle::Kmt(handle) => Self::Kmt(handle),
            UpdateSharedHandle::Nt(handle) => Self::Nt(handle),
            UpdateSharedHandle::None => Self::None,
        }
    }
}

#[napi(object)]
pub struct CopyRect {
    pub dst_x: u32,
    pub dst_y: u32,
    pub src: Rect,
}

impl From<CopyRect> for ty::CopyRect {
    fn from(val: CopyRect) -> Self {
        ty::CopyRect {
            dst_x: val.dst_x,
            dst_y: val.dst_y,
            src: ty::Rect {
                x: val.src.x,
                y: val.src.y,
                width: val.src.width,
                height: val.src.height,
            },
        }
    }
}

#[napi(object)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn create_adapter_by_luid(luid: GpuLuid) -> anyhow::Result<Option<IDXGIAdapter>> {
    let factory =
        unsafe { CreateDXGIFactory1::<IDXGIFactory1>().context("failed to create DXGI factory")? };

    let luid = LUID {
        LowPart: luid.low,
        HighPart: luid.high,
    };
    let mut i = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters(i) } {
        i += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc() }) else {
            continue;
        };

        if desc.AdapterLuid == luid {
            return Ok(Some(adapter));
        }
    }

    Ok(None)
}
