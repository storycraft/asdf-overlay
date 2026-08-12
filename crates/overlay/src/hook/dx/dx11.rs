use asdf_overlay_event::{RenderApi, SurfaceInfo};
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
    interop::DxInterop,
    renderer::dx11::Dx11Renderer,
    surface::{SurfaceState, Surfaces},
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
    if state.info.api != RenderApi::Direct3D11 {
        trace!("ignoring Direct3D11 rendering");
        return;
    }

    let Some(size) = state.texture_size() else {
        return;
    };

    let position = state.position();
    let screen = state.size();
    _ = with_or_init_renderer_data(swapchain, move |data| {
        trace!("using dx11 renderer");

        if state.texture.take_update() {
            data.renderer.update_texture(
                state
                    .texture
                    .get()
                    .as_ref()
                    .map(|surface| surface.shared_handle()),
            );
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
    let desc = unsafe { swapchain.GetDesc1() }?;
    let window_id = unsafe { swapchain.GetHwnd() }
        .ok()
        .map(|hwnd| hwnd.0 as u32);

    let interop = DxInterop::new(adapter.as_ref())?;
    let gpu_id = interop.gpu_id;
    SurfaceState::new(
        interop,
        (desc.Width, desc.Height),
        SurfaceInfo {
            api: RenderApi::Direct3D11,
            window_id,
            gpu_id,
        },
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
