pub mod cx;

use core::num::NonZeroU32;

use asdf_overlay_common::request::UpdateSharedHandle;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::{
    backend::render::cx::DrawContext,
    interop::DxInterop,
    renderer::{dx9::Dx9Renderer, dx11::Dx11Renderer, dx12::Dx12Renderer, vulkan::VulkanRenderer},
    surface::OverlaySurface,
};

pub enum Renderer {
    Dx12(Option<Dx12Renderer>),
    Dx11(Option<Dx11Renderer>),
    Dx9(Option<Dx9Renderer>),
    Opengl,
    Vulkan(Option<Box<VulkanRenderer>>),
}

pub struct RenderData {
    pub interop: DxInterop,

    pub position: (i32, i32),
    pub window_size: (u32, u32),
    pub surface: SurfaceState,
    pub renderer: Option<Renderer>,
    pub cx: DrawContext,
}

impl RenderData {
    pub fn new(interop: DxInterop, window_size: (u32, u32)) -> Self {
        Self {
            interop,
            surface: SurfaceState::new(),
            position: (0, 0),
            window_size,
            renderer: None,
            cx: DrawContext::new(),
        }
    }

    pub fn update_surface(&mut self, handle: Option<NonZeroU32>) -> anyhow::Result<()> {
        self.surface.update(&self.interop.device, handle)?;
        Ok(())
    }

    pub fn set_surface_updated(&mut self) {
        self.surface.updated = true;
    }
}

pub struct SurfaceState {
    inner: Option<OverlaySurface>,
    updated: bool,
}

impl SurfaceState {
    const fn new() -> Self {
        Self {
            inner: None,
            updated: false,
        }
    }

    #[inline]
    pub const fn get(&self) -> Option<&OverlaySurface> {
        self.inner.as_ref()
    }

    fn update(&mut self, device: &ID3D11Device, handle: Option<NonZeroU32>) -> anyhow::Result<()> {
        self.updated = true;
        self.inner.take();

        let Some(handle) = handle else {
            return Ok(());
        };

        self.inner = Some(OverlaySurface::open_shared(device, handle.get())?);
        Ok(())
    }

    #[inline]
    pub fn take_update(&mut self) -> Option<UpdateSharedHandle> {
        if self.updated {
            self.updated = false;
            Some(UpdateSharedHandle {
                handle: self.get().map(|surface| surface.shared_handle()),
            })
        } else {
            None
        }
    }

    #[inline]
    pub fn invalidate_update(&mut self) -> bool {
        if self.updated {
            self.updated = false;
            true
        } else {
            false
        }
    }
}
