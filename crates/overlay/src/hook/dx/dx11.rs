use asdf_overlay_common::request::UpdateSharedHandle;
use tracing::debug;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain1;

use crate::backend::{Backends, renderers::Renderer};

#[tracing::instrument]
pub fn cleanup_dx11_swapchain(swapchain: &IDXGISwapChain1) {
    debug!("dx11 renderer cleanup");
    let hwnd = unsafe { swapchain.GetHwnd() }.ok();

    let Some(hwnd) = hwnd else {
        return;
    };

    // We don't know if they are trying cleaning up entire device, so cleanup everything
    _ = Backends::with_backend(hwnd, |backend| {
        let Some(Renderer::Dx11(ref mut renderer)) = backend.renderer else {
            return;
        };

        if let Some(mut renderer) = renderer.take() {
            if let Some(handle) = renderer.take_texture() {
                backend.pending_handle = Some(UpdateSharedHandle {
                    handle: Some(handle),
                });
            }
        }
        backend.cx.dx11.take();
    });
}
