use core::{ffi::c_void, ptr};

use anyhow::Context;
use scopeguard::defer;
use tracing::{debug, trace};
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
            },
            Direct3D12::ID3D12Device,
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_PRESENT_TEST,
                DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
                IDXGIFactory1, IDXGISwapChain, IDXGISwapChain1, IDXGISwapChain3,
            },
        },
    },
    core::{BOOL, HRESULT, IUnknown, Interface},
};

use crate::{
    app::Overlay,
    backend::{Backends, WindowBackend},
    renderer::{dx11::Dx11Renderer, dx12::Dx12Renderer},
};

use super::{
    HOOK,
    dx12::{self, get_queue_for},
};

#[tracing::instrument(skip(backend))]
fn draw_overlay(backend: &mut WindowBackend, swapchain: &IDXGISwapChain) {
    let device = unsafe { swapchain.GetDevice::<IUnknown>() }.unwrap();

    let screen = backend.size;
    if let Ok(device) = device.cast::<ID3D12Device>() {
        let swapchain = swapchain.cast::<IDXGISwapChain3>().unwrap();

        if let Some(queue) = get_queue_for(&device) {
            let renderer = backend.renderer.dx12.get_or_insert_with(|| {
                debug!("initializing dx12 renderer");
                Dx12Renderer::new(&device, &queue, &swapchain).expect("renderer creation failed")
            });

            if let Some(shared) = backend.pending_handle.take() {
                renderer.update_texture(shared);
            }

            let size = renderer.size();
            let position = Overlay::with(|overlay| {
                overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
            });
            trace!("using dx12 renderer");
            let _res = renderer.draw(&device, &swapchain, &queue, position, screen);
            trace!("dx12 render: {:?}", _res);
        }
    } else if let Ok(device) = device.cast::<ID3D11Device1>() {
        let cx = unsafe { device.GetImmediateContext1().unwrap() };

        let state = backend.cx.dx11.get_or_insert_with(|| {
            let mut state = None;
            unsafe {
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
        let renderer = backend.renderer.dx11.get_or_insert_with(|| {
            debug!("initializing dx11 renderer");
            Dx11Renderer::new(&device).expect("renderer creation failed")
        });

        if let Some(shared) = backend.pending_handle.take() {
            renderer.update_texture(shared);
        }

        let size = renderer.size();
        let position = Overlay::with(|overlay| {
            overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
        });

        let _res = renderer.draw(&device, &cx, swapchain, position, screen);
        trace!("dx11 render: {:?}", _res);
    }
}

#[tracing::instrument]
fn cleanup_state(swapchain: &IDXGISwapChain1, hwnd: Option<HWND>) {
    if let Some(hwnd) = hwnd {
        _ = Backends::with_backend(hwnd, |backend| {
            backend.cx.dx11.take();
        });
    }

    dx12::clear();
    trace!("cleanedup render states");
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    trace!("Present called");

    if flags & DXGI_PRESENT_TEST != DXGI_PRESENT_TEST {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
            Backends::with_or_init_backend(hwnd, |backend| {
                draw_overlay(backend, swapchain);
            })
            .expect("Backends::with_backend failed");
        }
    }

    let present = HOOK.present.get().unwrap();
    unsafe { present.original_fn()(this, sync_interval, flags) }
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_create_swapchain(
    this: *mut c_void,
    device: *mut c_void,
    desc: *const DXGI_SWAP_CHAIN_DESC,
    out_swap_chain: *mut *mut c_void,
) -> HRESULT {
    trace!("CreateSwapChain called");

    let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this) }.unwrap();
    let desc = unsafe { &*desc };
    let hwnd = desc.OutputWindow;

    if !hwnd.is_invalid() {
        cleanup_state(swapchain, if hwnd.is_invalid() { None } else { Some(hwnd) });
    }

    let create_swapchain = HOOK.create_swapchain.get().unwrap();
    unsafe { create_swapchain.original_fn()(this, device, desc, out_swap_chain) }
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_resize_buffers(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    flags: u32,
) -> HRESULT {
    trace!("ResizeBuffers called");

    let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this) }.unwrap();
    let hwnd = unsafe { swapchain.GetHwnd().ok() };
    cleanup_state(swapchain, hwnd);

    let resize_buffers = HOOK.resize_buffers.get().unwrap();
    let res =
        unsafe { resize_buffers.original_fn()(this, buffer_count, width, height, format, flags) };
    if res.is_err() {
        return res;
    }

    if let Some(hwnd) = hwnd {
        Backends::with_or_init_backend(hwnd, |backend| {
            if let Some(ref mut renderer) = backend.renderer.dx12 {
                let device = unsafe { swapchain.GetDevice::<ID3D12Device>() }.unwrap();
                renderer.resize(&device, swapchain);
            }
        })
        .unwrap();
    }

    res
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    trace!("Present1 called");

    if flags & DXGI_PRESENT_TEST != DXGI_PRESENT_TEST {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        if let Ok(hwnd) = unsafe { swapchain.GetHwnd() } {
            Backends::with_or_init_backend(hwnd, |backend| {
                draw_overlay(backend, swapchain);
            })
            .expect("Backends::with_backend failed");
        }
    }

    let present1 = HOOK.present1.get().unwrap();
    unsafe { present1.original_fn()(this, sync_interval, flags, present_params) }
}

pub type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;
pub type Present1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    DXGI_PRESENT,
    *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT;

pub type CreateSwapChainFn = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    *const DXGI_SWAP_CHAIN_DESC,
    *mut *mut c_void,
) -> HRESULT;

pub type ResizeBuffersFn =
    unsafe extern "system" fn(*mut c_void, u32, u32, u32, DXGI_FORMAT, u32) -> HRESULT;

pub struct DxgiFunctions {
    pub present: PresentFn,
    pub present1: Option<Present1Fn>,

    pub create_swapchain: CreateSwapChainFn,

    pub resize_buffers: ResizeBuffersFn,
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

        let dxgi_factory_vtable = Interface::vtable(&*factory);
        let create_swapchain = dxgi_factory_vtable.CreateSwapChain;
        debug!(
            "IDXGIFactory::CreateSwapChain found: {:p}",
            create_swapchain
        );

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
        debug!("IDXGISwapChain::ResizeBuffers found: {:p}", present);

        let present1 = swapchain.cast::<IDXGISwapChain1>().ok().map(|swapchain1| {
            let present1 = Interface::vtable(&swapchain1).Present1;
            debug!("IDXGISwapChain1::Present1 found: {:p}", present1);
            present1
        });

        Ok(DxgiFunctions {
            present,
            present1,

            create_swapchain,

            resize_buffers,
        })
    }
}
