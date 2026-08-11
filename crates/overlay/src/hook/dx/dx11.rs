use dashmap::Entry;
use once_cell::sync::Lazy;
use scopeguard::defer;
use tracing::{Level, info, trace};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D11::{
            D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED, D3D11_CREATE_DEVICE_SINGLETHREADED,
            D3D11_SDK_VERSION, ID3D11Device, ID3D11Device1, ID3D11Texture2D,
            ID3DDeviceContextState,
        },
        Dxgi::{IDXGIDevice, IDXGISwapChain1},
    },
    core::Interface,
};

use crate::{
    hook::dx::dxgi::callback::register_swapchain_destruction_callback,
    renderer::dx11::Dx11Renderer,
    surface::{Renderer, SurfaceState, Surfaces},
    types::IntDashMap,
};

/// Mapping from [`IDXGISwapChain1`] to [`RendererData`].
static RENDERERS: Lazy<IntDashMap<usize, RendererData>> = Lazy::new(IntDashMap::default);

struct RendererData {
    renderer: Dx11Renderer,
    state: ID3DDeviceContextState,
}

#[inline]
fn with_or_init_renderer_data<R>(
    swapchain: &IDXGISwapChain1,
    f: impl FnOnce(&mut RendererData) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let mut data = match RENDERERS.entry(swapchain.as_raw() as _) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            info!("initializing dx11 renderer");
            let device = unsafe { swapchain.GetDevice::<ID3D11Device1>()? };

            let state = unsafe {
                let mut state = None;
                let flag = if device.GetCreationFlags() & D3D11_CREATE_DEVICE_SINGLETHREADED.0 != 0
                {
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

                state.unwrap()
            };

            let ref_mut = entry.insert(RendererData {
                renderer: Dx11Renderer::new(&device)?,
                state,
            });
            register_swapchain_destruction_callback(swapchain, cleanup_swapchain);

            ref_mut
        }
    };

    f(&mut data)
}

pub fn draw_overlay(state: &SurfaceState, device: &ID3D11Device1, swapchain: &IDXGISwapChain1) {
    if state.renderer != Renderer::Dx11 {
        trace!("ignoring dx11 rendering");
        return;
    }

    let Some(size) = state.surface_size() else {
        return;
    };

    let update = state.surface.take_update();
    let position = state.position();
    let screen = state.size();
    _ = with_or_init_renderer_data(swapchain, move |data| {
        trace!("using dx11 renderer");

        if let Some(update) = update {
            data.renderer.update_texture(update);
        }

        let cx = unsafe { device.GetImmediateContext1().unwrap() };
        let mut prev_state = None;
        unsafe {
            cx.SwapDeviceContextState(&data.state, Some(&mut prev_state));
        }

        let prev_state = prev_state.unwrap();
        defer!(unsafe {
            cx.SwapDeviceContextState(&prev_state, None);
        });

        let back_buffer = unsafe { swapchain.GetBuffer::<ID3D11Texture2D>(0) }
            .expect("failed to get dx11 backbuffer");
        let mut rtv = None;
        unsafe { device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv)) }
            .expect("failed to create rtv");
        let rtv = rtv.unwrap();

        unsafe { cx.OMSetRenderTargets(Some(&[Some(rtv)]), None) };
        defer!(unsafe { cx.OMSetRenderTargets(None, None) });

        let res = data.renderer.draw(device, &cx, position, size, screen);
        trace!("dx11 render: {:?}", res);
        res
    });
}

pub(super) fn setup_fn(
    device: &ID3D11Device1,
    swapchain: &IDXGISwapChain1,
) -> anyhow::Result<SurfaceState> {
    let adapter = unsafe { device.cast::<IDXGIDevice>().unwrap().GetAdapter().ok() };
    let desc = unsafe { swapchain.GetDesc() }?;

    SurfaceState::new(
        adapter.as_ref(),
        (desc.BufferDesc.Width, desc.BufferDesc.Height),
        Renderer::Dx11,
    )
}

#[tracing::instrument(level = Level::TRACE)]
fn cleanup_swapchain(swapchain: usize) {
    if RENDERERS.remove(&swapchain).is_none() {
        return;
    };
    info!("dx11 renderer cleanup");

    Surfaces::cleanup_state(swapchain as _);
}
