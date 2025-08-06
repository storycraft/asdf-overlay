use core::{ffi::c_void, ptr};

use anyhow::Context;
use scopeguard::defer;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_11_0,
            Direct3D10::{
                D3D10_DRIVER_TYPE_HARDWARE, D3D10_SDK_VERSION, D3D10CreateDeviceAndSwapChain,
            },
            Direct3D11::{
                D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED,
                D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_SDK_VERSION, ID3D11Device, ID3D11Device1,
                ID3D11Texture2D,
            },
            Direct3D12::ID3D12Device,
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_PRESENT_TEST,
                DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
                IDXGIFactory1, IDXGISwapChain1, IDXGISwapChain3,
            },
        },
    },
    core::{BOOL, HRESULT, IUnknown, Interface},
};

use crate::{
    app::OverlayIpc,
    backend::{
        Backends, WindowBackend,
        render::{
            Renderer,
            cx::{callback::register_swapchain_destruction_callback, dx12::RtvDescriptors},
        },
    },
    hook::dx::{dx11, dx12},
    renderer::{dx11::Dx11Renderer, dx12::Dx12Renderer},
};

use super::{HOOK, dx12::get_queue_for};

#[tracing::instrument(skip(backend))]
fn draw_overlay(backend: &WindowBackend, swapchain: &IDXGISwapChain1) {
    let device = unsafe { swapchain.GetDevice::<IUnknown>() }.unwrap();
    if let Ok(device) = device.cast::<ID3D12Device>() {
        draw_dx12_overlay(backend, &device, swapchain);
    } else if let Ok(device) = device.cast::<ID3D11Device1>() {
        draw_dx11_overlay(backend, &device, swapchain);
    }
}

#[inline]
fn draw_dx12_overlay(backend: &WindowBackend, device: &ID3D12Device, swapchain: &IDXGISwapChain1) {
    let render = &mut *backend.render.lock();
    let renderer = match render.renderer {
        Some(Renderer::Dx12(ref mut renderer)) => renderer,

        // drawing on opengl with dxgi swapchain can cause deadlock
        Some(Renderer::Opengl) => {
            render.renderer = Some(Renderer::Dx12(None));
            debug!("switching from opengl to dx12 render");
            // skip drawing on render changes
            return;
        }
        // use dxgi swapchain instead
        Some(Renderer::Vulkan(_)) => {
            render.renderer = Some(Renderer::Dx12(None));
            debug!("switching from vulkan to dx12 render");
            return;
        }
        Some(_) => {
            trace!("ignoring dx12 rendering");
            return;
        }
        None => {
            render.renderer = Some(Renderer::Dx12(None));
            // skip drawing on renderer check
            return;
        }
    };

    let swapchain = swapchain.cast::<IDXGISwapChain3>().unwrap();
    let Some(queue) = get_queue_for(&device) else {
        return;
    };

    let renderer = renderer.get_or_insert_with(|| {
        debug!("initializing dx12 renderer");
        register_swapchain_destruction_callback(&swapchain, dx12::cleanup_swapchain);
        Dx12Renderer::new(&device, &queue, &swapchain).expect("renderer creation failed")
    });
    let rtv = render
        .cx
        .dx12
        .get_or_insert_with(|| RtvDescriptors::new(&device).expect("failed to create dx12 rtv"));

    if let Some(update) = render.surface.take_update() {
        renderer.update_texture(update);
    }

    let Some(surface) = render.surface.get() else {
        return;
    };

    let size = surface.size();
    trace!("using dx12 renderer");
    let backbuffer_index = unsafe { swapchain.GetCurrentBackBufferIndex() };
    let res = rtv.with_next_swapchain(&device, &swapchain, backbuffer_index as _, |desc| {
        renderer.draw(
            &device,
            &swapchain,
            backbuffer_index,
            desc,
            &queue,
            render.position,
            size,
            render.window_size,
        )
    });
    trace!("dx12 render: {:?}", res);
}

#[inline]
fn draw_dx11_overlay(backend: &WindowBackend, device: &ID3D11Device1, swapchain: &IDXGISwapChain1) {
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
        register_swapchain_destruction_callback(swapchain, dx11::cleanup_swapchain);
        Dx11Renderer::new(&device).expect("renderer creation failed")
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
        let _res = renderer.draw(&device, &cx, render.position, size, render.window_size);
        trace!("dx11 render: {:?}", _res);
    }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    trace!("Present called");

    if OverlayIpc::connected() && flags & DXGI_PRESENT_TEST != DXGI_PRESENT_TEST {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
            if let Err(_err) = Backends::with_or_init_backend(hwnd, |backend| {
                draw_overlay(backend, swapchain);
            }) {
                error!("Backends::with_or_init_backend failed. err: {:?}", _err);
            }
        }
    }

    unsafe { HOOK.present.wait().original_fn()(this, sync_interval, flags) }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    trace!("Present1 called");

    if OverlayIpc::connected() && flags & DXGI_PRESENT_TEST != DXGI_PRESENT_TEST {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
            if let Err(_err) = Backends::with_or_init_backend(hwnd, |backend| {
                draw_overlay(backend, swapchain);
            }) {
                error!("Backends::with_or_init_backend failed. err: {:?}", _err);
            }
        }
    }

    unsafe { HOOK.present1.wait().original_fn()(this, sync_interval, flags, present_params) }
}

fn resize_swapchain(backend: &WindowBackend) {
    let render = &mut *backend.render.lock();
    let Some(ref renderer) = render.renderer else {
        return;
    };

    if let Renderer::Dx12(_) = *renderer {
        if let Some(ref mut rtv) = render.cx.dx12 {
            // invalidate old rtv descriptors
            rtv.reset();
        }
    }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_resize_buffers(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    flags: u32,
) -> HRESULT {
    trace!("ResizeBuffers called");

    let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
    if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
        _ = Backends::with_backend(hwnd, resize_swapchain);
    }

    unsafe {
        HOOK.resize_buffers.wait().original_fn()(this, buffer_count, width, height, format, flags)
    }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_resize_buffers1(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    flags: u32,
    creation_node_mask: *const u32,
    present_queue: *const *mut c_void,
) -> HRESULT {
    trace!("ResizeBuffers1 called");

    let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
    if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
        _ = Backends::with_backend(hwnd, resize_swapchain);
    }

    unsafe {
        HOOK.resize_buffers1.wait().original_fn()(
            this,
            buffer_count,
            width,
            height,
            format,
            flags,
            creation_node_mask,
            present_queue,
        )
    }
}

pub type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;
pub type Present1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    DXGI_PRESENT,
    *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT;

pub type ResizeBuffersFn =
    unsafe extern "system" fn(*mut c_void, u32, u32, u32, DXGI_FORMAT, u32) -> HRESULT;
pub type ResizeBuffers1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    u32,
    u32,
    DXGI_FORMAT,
    u32,
    *const u32,
    *const *mut c_void,
) -> HRESULT;

pub struct DxgiFunctions {
    pub present: PresentFn,
    pub present1: Option<Present1Fn>,
    pub resize_buffers: ResizeBuffersFn,
    pub resize_buffers1: Option<ResizeBuffers1Fn>,
}

/// Get pointer to dxgi functions
#[tracing::instrument]
pub fn get_dxgi_addr(dummy_hwnd: HWND) -> anyhow::Result<DxgiFunctions> {
    unsafe {
        let factory = CreateDXGIFactory1::<IDXGIFactory1>()?;
        let adapter = factory.EnumAdapters1(0)?;

        let desc = DXGI_SWAP_CHAIN_DESC {
            BufferCount: 2,
            BufferDesc: DXGI_MODE_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                ..Default::default()
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            OutputWindow: dummy_hwnd,
            Windowed: BOOL(1),
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            ..Default::default()
        };

        let mut swapchain = None;
        let mut device = None;

        D3D10CreateDeviceAndSwapChain(
            &adapter,
            D3D10_DRIVER_TYPE_HARDWARE,
            HMODULE(ptr::null_mut()),
            0,
            D3D10_SDK_VERSION,
            Some(&desc),
            Some(&mut swapchain),
            Some(&mut device),
        )?;
        let swapchain = swapchain.context("SwapChain creation failed")?;

        let swapchain_vtable = Interface::vtable(&swapchain);
        let present = swapchain_vtable.Present;
        debug!("IDXGISwapChain::Present found: {:p}", present);

        let resize_buffers = swapchain_vtable.ResizeBuffers;
        debug!("IDXGISwapChain::ResizeBuffers found: {:p}", resize_buffers);

        let present1 = swapchain.cast::<IDXGISwapChain1>().ok().map(|swapchain1| {
            let present1 = Interface::vtable(&swapchain1).Present1;
            debug!("IDXGISwapChain1::Present1 found: {:p}", present1);
            present1
        });

        let resize_buffers1 = swapchain.cast::<IDXGISwapChain3>().ok().map(|swapchain3| {
            let resize_buffers1 = Interface::vtable(&swapchain3).ResizeBuffers1;
            debug!(
                "IDXGISwapChain3::ResizeBuffers1 found: {:p}",
                resize_buffers1
            );
            resize_buffers1
        });

        Ok(DxgiFunctions {
            present,
            resize_buffers,
            present1,
            resize_buffers1,
        })
    }
}
