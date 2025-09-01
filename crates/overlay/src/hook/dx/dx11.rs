use scopeguard::defer;
use tracing::{debug, trace};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D11::{
            D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED, D3D11_CREATE_DEVICE_SINGLETHREADED,
            D3D11_SDK_VERSION, ID3D11Device, ID3D11Device1, ID3D11Texture2D,
        },
        Dxgi::IDXGISwapChain1,
    },
    core::Interface,
};

use crate::{
    backend::{Backends, WindowBackend, render::Renderer},
    hook::dx::dxgi::callback::register_swapchain_destruction_callback,
    renderer::dx11::Dx11Renderer,
};

pub fn draw_overlay(backend: &WindowBackend, device: &ID3D11Device1, swapchain: &IDXGISwapChain1) {
    let render = &mut *backend.render.lock();
    let renderer = match render.renderer {
        Some(Renderer::Dx11(ref mut renderer)) => renderer,

        // drawing on opengl with dxgi swapchain can cause deadlock
        Some(Renderer::Opengl) => {
            render.renderer = Some(Renderer::Dx11(None));
            debug!("switching from opengl to dx11 render");
            // skip drawing on render changes
            return;
        }
        Some(_) => {
            trace!("ignoring dx11 rendering");
            return;
        }
        None => {
            render.renderer = Some(Renderer::Dx11(None));
            // skip drawing on renderer check
            return;
        }
    };

    let cx = unsafe { device.GetImmediateContext1().unwrap() };
    let state = render.cx.dx11.get_or_insert_with(|| {
        let mut state = None;
        unsafe {
            let flag = if device.GetCreationFlags() & D3D11_CREATE_DEVICE_SINGLETHREADED.0 != 0 {
                D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED.0 as u32
            } else {
                0
            };

            device
                .CreateDeviceContextState(
                    flag,
                    &[D3D_FEATURE_LEVEL_11_0],
                    D3D11_SDK_VERSION,
                    &ID3D11Device::IID,
                    None,
                    Some(&mut state),
                )
                .expect("CreateDeviceContextState failed");
        }

        state.unwrap()
    });

    let mut prev_state = None;
    unsafe {
        cx.SwapDeviceContextState(&*state, Some(&mut prev_state));
    }
    let prev_state = prev_state.unwrap();
    defer!(unsafe {
        cx.SwapDeviceContextState(&prev_state, None);
    });

    trace!("using dx11 renderer");
    let renderer = renderer.get_or_insert_with(|| {
        debug!("initializing dx11 renderer");
        register_swapchain_destruction_callback(swapchain, cleanup_swapchain);
        Dx11Renderer::new(device).expect("renderer creation failed")
    });

    if let Some(update) = render.surface.take_update() {
        renderer.update_texture(update);
    }

    let Some(surface) = render.surface.get() else {
        return;
    };
    let size = surface.size();
    {
        let back_buffer = unsafe { swapchain.GetBuffer::<ID3D11Texture2D>(0) }
            .expect("failed to get dx11 backbuffer");
        let mut rtv = None;
        unsafe { device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv)) }
            .expect("failed to create rtv");
        let rtv = rtv.unwrap();

        unsafe { cx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None) };
        defer!(unsafe { cx.OMSetRenderTargets(None, None) });
        let _res = renderer.draw(device, &cx, render.position, size, render.window_size);
        trace!("dx11 render: {:?}", _res);
    }
}

#[tracing::instrument]
fn cleanup_swapchain(swapchain: &IDXGISwapChain1) {
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
