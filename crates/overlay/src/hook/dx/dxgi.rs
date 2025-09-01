pub mod callback;

use core::{ffi::c_void, ptr};

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D10::{
                D3D10_DRIVER_TYPE_HARDWARE, D3D10_SDK_VERSION, D3D10CreateDeviceAndSwapChain,
            },
            Direct3D11::ID3D11Device1,
            Direct3D12::ID3D12Device,
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_PRESENT_TEST,
                DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
                IDXGIAdapter, IDXGIDevice, IDXGIFactory1, IDXGIFactory4, IDXGISwapChain1,
                IDXGISwapChain3,
            },
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    backend::Backends,
    event_sink::OverlayEventSink,
    hook::dx::{dx11, dx12},
};

#[tracing::instrument]
fn draw_overlay(swapchain: &IDXGISwapChain1) {
    let Ok(hwnd) = (unsafe { swapchain.GetHwnd() }) else {
        return;
    };

    if let Ok(device) = unsafe { swapchain.GetDevice::<ID3D12Device>() } {
        if let Err(_err) = Backends::with_or_init_backend(
            hwnd.0 as _,
            || {
                let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory4>() }.ok()?;
                let luid = unsafe { device.GetAdapterLuid() };
                unsafe { factory.EnumAdapterByLuid::<IDXGIAdapter>(luid) }.ok()
            },
            |backend| {
                dx12::draw_overlay(
                    backend,
                    &device,
                    &swapchain.cast::<IDXGISwapChain3>().unwrap(),
                );
            },
        ) {
            error!("Backends::with_or_init_backend failed. err: {:?}", _err);
        }
    } else if let Ok(device) = unsafe { swapchain.GetDevice::<ID3D11Device1>() } {
        if let Err(_err) = Backends::with_or_init_backend(
            hwnd.0 as _,
            || unsafe { device.cast::<IDXGIDevice>().unwrap().GetAdapter().ok() },
            |backend| {
                dx11::draw_overlay(backend, &device, swapchain);
            },
        ) {
            error!("Backends::with_or_init_backend failed. err: {:?}", _err);
        }
    }
}

#[tracing::instrument]
extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    trace!("Present called");

    if !flags.contains(DXGI_PRESENT_TEST) && OverlayEventSink::connected() {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        draw_overlay(swapchain);
    }

    unsafe { HOOK.present.wait().original_fn()(this, sync_interval, flags) }
}

#[tracing::instrument]
extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    trace!("Present1 called");

    if !flags.contains(DXGI_PRESENT_TEST) && OverlayEventSink::connected() {
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
        draw_overlay(swapchain);
    }

    unsafe { HOOK.present1.wait().original_fn()(this, sync_interval, flags, present_params) }
}

fn resize_swapchain(swapchain: &IDXGISwapChain1) {
    dx12::resize_swapchain(swapchain);
}

#[tracing::instrument]
extern "system" fn hooked_resize_buffers(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
    flags: u32,
) -> HRESULT {
    trace!("ResizeBuffers called");

    let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() };
    resize_swapchain(swapchain);

    unsafe {
        HOOK.resize_buffers.wait().original_fn()(this, buffer_count, width, height, format, flags)
    }
}

#[tracing::instrument]
extern "system" fn hooked_resize_buffers1(
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
    resize_swapchain(swapchain);

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

struct Hook {
    present: OnceCell<DetourHook<PresentFn>>,
    present1: OnceCell<DetourHook<Present1Fn>>,
    resize_buffers: OnceCell<DetourHook<ResizeBuffersFn>>,
    resize_buffers1: OnceCell<DetourHook<ResizeBuffers1Fn>>,
}

static HOOK: Hook = Hook {
    present: OnceCell::new(),
    present1: OnceCell::new(),
    resize_buffers: OnceCell::new(),
    resize_buffers1: OnceCell::new(),
};

pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let dxgi_functions = get_addr(dummy_hwnd).context("failed to load dxgi addrs")?;

    debug!("hooking IDXGISwapChain::ResizeBuffers");
    HOOK.resize_buffers.get_or_try_init(|| unsafe {
        DetourHook::attach(dxgi_functions.resize_buffers, hooked_resize_buffers as _)
    })?;

    if let Some(resize_buffers1) = dxgi_functions.resize_buffers1 {
        debug!("hooking IDXGISwapChain3::ResizeBuffers1");
        HOOK.resize_buffers1.get_or_try_init(|| unsafe {
            DetourHook::attach(resize_buffers1, hooked_resize_buffers1 as _)
        })?;
    }

    debug!("hooking IDXGISwapChain::Present");
    HOOK.present.get_or_try_init(|| unsafe {
        DetourHook::attach(dxgi_functions.present, hooked_present as _)
    })?;

    if let Some(present1) = dxgi_functions.present1 {
        debug!("hooking IDXGISwapChain1::Present1");
        HOOK.present1
            .get_or_try_init(|| unsafe { DetourHook::attach(present1, hooked_present1 as _) })?;
    }
    Ok(())
}

pub struct DxgiFunctions {
    pub present: PresentFn,
    pub present1: Option<Present1Fn>,
    pub resize_buffers: ResizeBuffersFn,
    pub resize_buffers1: Option<ResizeBuffers1Fn>,
}

/// Get pointer to dxgi functions
#[tracing::instrument]
pub fn get_addr(dummy_hwnd: HWND) -> anyhow::Result<DxgiFunctions> {
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
