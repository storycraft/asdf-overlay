use core::{ffi::c_void, mem, ptr};

use anyhow::Context;
use scopeguard::defer;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D10::{
                D3D10_DRIVER_TYPE_HARDWARE, D3D10_SDK_VERSION, D3D10CreateDeviceAndSwapChain,
                ID3D10Device,
            },
            Direct3D11::ID3D11Device,
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
    renderer::{Renderers, dx11::Dx11Renderer, dx12::Dx12Renderer},
    util::get_client_size,
};

use super::{
    HOOK,
    dx12::{self, get_queue_for},
};

pub type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;
pub type Present1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    DXGI_PRESENT,
    *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT;
pub type ResizeBuffersFn =
    unsafe extern "system" fn(*mut c_void, u32, u32, u32, DXGI_FORMAT, u32) -> HRESULT;

#[tracing::instrument]
pub unsafe extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    let Some(ref present) = HOOK.read().present else {
        return HRESULT(0);
    };
    trace!("Present called");

    Renderers::with(move |renderers| {
        let call_present = move || unsafe {
            mem::transmute::<*const (), PresentFn>(present.original_fn())(
                this,
                sync_interval,
                flags,
            )
        };

        let test = flags & DXGI_PRESENT_TEST == DXGI_PRESENT_TEST;
        if !test {
            draw_overlay(
                renderers,
                unsafe { IDXGISwapChain::from_raw_borrowed(&this).unwrap() },
                call_present,
            )
        } else {
            call_present()
        }
    })
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
    let Some(ref resize_buffers) = HOOK.read().resize_buffers else {
        return HRESULT(0);
    };
    trace!("ResizeBuffers called");

    dx12::clear();

    let res = unsafe {
        mem::transmute::<*const (), ResizeBuffersFn>(resize_buffers.original_fn())(
            this,
            buffer_count,
            width,
            height,
            format,
            flags,
        )
    };
    if res.is_err() {
        return res;
    }

    Renderers::with(|renderer| {
        if let Some(ref mut renderer) = renderer.dx12 {
            let (device, swapchain) = unsafe {
                let swapchain = IDXGISwapChain::from_raw_borrowed(&this).unwrap();
                let device = swapchain.GetDevice::<ID3D12Device>().unwrap();

                (device, swapchain)
            };

            renderer.resize(&device, swapchain);
        }
    });

    res
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    let Some(ref present1) = HOOK.read().present1 else {
        return HRESULT(0);
    };
    trace!("Present1 called");

    Renderers::with(move |renderers| {
        let call_present1 = move || unsafe {
            mem::transmute::<*const (), Present1Fn>(present1.original_fn())(
                this,
                sync_interval,
                flags,
                present_params,
            )
        };

        let test = flags & DXGI_PRESENT_TEST == DXGI_PRESENT_TEST;
        if !test {
            draw_overlay(
                renderers,
                unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() },
                call_present1,
            )
        } else {
            call_present1()
        }
    })
}

#[tracing::instrument(skip(renderers, call_present))]
fn draw_overlay(
    renderers: &mut Renderers,
    swapchain: &IDXGISwapChain,
    call_present: impl FnOnce() -> HRESULT,
) -> HRESULT {
    let device = unsafe { swapchain.GetDevice::<IUnknown>() }.unwrap();

    let screen = {
        let Ok(desc) = (unsafe { swapchain.GetDesc() }) else {
            return call_present();
        };

        get_client_size(desc.OutputWindow).unwrap_or_default()
    };

    if let Ok(device) = device.cast::<ID3D12Device>() {
        let swapchain = swapchain.cast::<IDXGISwapChain3>().unwrap();

        let renderer = renderers.dx12.get_or_insert_with(|| {
            debug!("initializing dx12 renderer");
            Dx12Renderer::new(&device, &swapchain).expect("renderer creation failed")
        });

        let position = Overlay::with(|overlay| {
            let size = renderer.size();
            overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
        });

        if let Some(queue) = get_queue_for(&device) {
            trace!("using dx12 renderer");
            _ = renderer.draw(&device, &swapchain, &queue, position, screen);
            defer!({
                _ = renderer.post_present(&swapchain);
            });

            return call_present();
        }
    } else if let Ok(device) = device.cast::<ID3D11Device>() {
        trace!("using dx11 renderer");
        let renderer = renderers.dx11.get_or_insert_with(|| {
            debug!("initializing dx11 renderer");
            Dx11Renderer::new(&device).expect("renderer creation failed")
        });
        let position = Overlay::with(|overlay| {
            let size = renderer.size();
            overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
        });

        _ = renderer.draw(&device, swapchain, position, screen);
    } else if let Ok(_) = device.cast::<ID3D10Device>() {
        trace!("using dx10 renderer");
    }

    call_present()
}

/// Get pointer to IDXGISwapChain::Present, IDXGISwapChain::ResizeBuffers and IDXGISwapChain1::Present1 by creating dummy swapchain
#[tracing::instrument]
pub fn get_dxgi_addr(
    dummy_hwnd: HWND,
) -> anyhow::Result<(PresentFn, ResizeBuffersFn, Option<Present1Fn>)> {
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
        debug!("IDXGISwapChain::ResizeBuffers found: {:p}", present);
        let present1 = swapchain.cast::<IDXGISwapChain1>().ok().map(|swapchain1| {
            let present1 = Interface::vtable(&swapchain1).Present1;
            debug!("IDXGISwapChain1::Present1 found: {:p}", present1);
            present1
        });

        Ok((present, resize_buffers, present1))
    }
}
