use core::num::NonZeroU32;

use anyhow::Context;
use asdf_overlay_client::{common::request, surface, ty};
use bytemuck::try_pod_read_unaligned;
use napi::bindgen_prelude::BufferSlice;
use napi_derive::napi;
use windows::Win32::{
    Foundation::LUID,
    Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
};

use crate::GpuLuid;

/// Represent a surface for overlay
#[napi]
pub struct OverlaySurface(surface::OverlaySurface);

#[napi]
impl OverlaySurface {
    /// Create a new overlay surface.
    #[napi(constructor)]
    pub fn new(luid: Option<GpuLuid>) -> anyhow::Result<Self> {
        let adapter = luid.map(create_adapter_by_luid).transpose()?.flatten();
        let surface = surface::OverlaySurface::new(adapter.as_ref())?;
        Ok(Self(surface))
    }

    /// Clear the surface.
    #[napi]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Update surface using D3D11 NT shared texture.
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

    /// Update surface using D3D11 KMT shared texture.
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

    /// Update surface using bitmap buffer. The size of overlay is `width x (data.byteLength / 4 / width)`
    #[napi]
    pub fn update_bitmap(
        &mut self,
        width: u32,
        data: BufferSlice,
    ) -> anyhow::Result<Option<UpdateSharedHandle>> {
        Ok(self.0.update_bitmap(width, &data)?.map(From::from))
    }
}

#[napi(object)]
pub struct UpdateSharedHandle {
    pub handle: Option<u32>,
}

impl From<request::UpdateSharedHandle> for UpdateSharedHandle {
    fn from(update: request::UpdateSharedHandle) -> Self {
        Self {
            handle: update.handle.map(|h| h.get()),
        }
    }
}

impl From<UpdateSharedHandle> for request::UpdateSharedHandle {
    fn from(val: UpdateSharedHandle) -> Self {
        request::UpdateSharedHandle {
            handle: val.handle.and_then(NonZeroU32::new),
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
