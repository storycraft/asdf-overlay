use tracing::debug;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain1;

use crate::backend::{Backends, render::Renderer};

#[tracing::instrument]
pub fn cleanup_swapchain(swapchain: &IDXGISwapChain1) {
    let hwnd = unsafe { swapchain.GetHwnd() }.ok();

    let Some(hwnd) = hwnd else {
        return;
    };

    // We don't know if they are trying clean up entire device, so cleanup everything
    _ = Backends::with_backend(hwnd.0 as _, |backend| {
        let render = &mut *backend.render.lock();
        let Some(Renderer::Dx11(ref mut renderer)) = render.renderer else {
            return;
        };
        debug!("dx11 renderer cleanup");

        renderer.take();
        render.cx.dx11.take();
        render.set_surface_updated();
    });
}
