use core::{ffi::c_void, mem, ptr};

use anyhow::Context;
use parking_lot::{Mutex, RwLock};
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
                Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC},
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_PRESENT_TEST,
                DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
                IDXGIFactory1, IDXGISwapChain, IDXGISwapChain1,
            },
        },
    },
    core::{BOOL, HRESULT, IUnknown, Interface},
};

use crate::{app::Overlay, renderer::dx11::Dx11Renderer, util::get_client_size};

use super::DetourHook;

type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;
type Present1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    DXGI_PRESENT,
    *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT;

struct Hook {
    present: Option<DetourHook>,
    present1: Option<DetourHook>,
}

static HOOK: RwLock<Hook> = RwLock::new(Hook {
    present: None,
    present1: None,
});

unsafe extern "system" fn hooked_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    let Some(ref present) = HOOK.read().present else {
        return HRESULT(0);
    };

    let test = flags & DXGI_PRESENT_TEST == DXGI_PRESENT_TEST;
    if !test {
        draw_overlay(unsafe { IDXGISwapChain::from_raw_borrowed(&this).unwrap() });
    }

    unsafe {
        mem::transmute::<*const (), PresentFn>(present.original_fn())(this, sync_interval, flags)
    }
}

unsafe extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    let Some(ref present1) = HOOK.read().present1 else {
        return HRESULT(0);
    };

    let test = flags & DXGI_PRESENT_TEST == DXGI_PRESENT_TEST;
    if !test {
        draw_overlay(unsafe { IDXGISwapChain1::from_raw_borrowed(&this).unwrap() });
    }

    unsafe {
        mem::transmute::<*const (), Present1Fn>(present1.original_fn())(
            this,
            sync_interval,
            flags,
            present_params,
        )
    }
}

pub static RENDERER: Renderers = Renderers {
    dx11: Mutex::new(None),
};

pub struct Renderers {
    pub dx11: Mutex<Option<Dx11Renderer>>,
}

fn draw_overlay(swapchain: &IDXGISwapChain) {
    let Ok(device) = (unsafe { swapchain.GetDevice::<IUnknown>() }) else {
        return;
    };

    let screen = {
        let Ok(desc) = (unsafe { swapchain.GetDesc() }) else {
            return;
        };

        get_client_size(desc.OutputWindow).unwrap_or_default()
    };

    if let Ok(_) = device.cast::<ID3D12Device>() {
    } else if let Ok(device) = device.cast::<ID3D11Device>() {
        let mut renderer = RENDERER.dx11.lock();
        let renderer = renderer
            .get_or_insert_with(|| Dx11Renderer::new(&device).expect("renderer creation failed"));

        _ = Overlay::with(|overlay| {
            let size = renderer.size();

            renderer.draw(
                &device,
                swapchain,
                overlay.calc_overlay_position((size.0 as _, size.1 as _), screen),
                screen,
            )?;

            Ok::<_, anyhow::Error>(())
        });
    } else if let Ok(_) = device.cast::<ID3D10Device>() {
    }
}

pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let (present, present1) = get_dxgi_addr(dummy_hwnd)?;
    let mut hook = HOOK.write();

    let present_hook = unsafe { DetourHook::attach(present as _, hooked_present as _)? };
    hook.present = Some(present_hook);

    if let Some(present1) = present1 {
        let present1_hook = unsafe { DetourHook::attach(present1 as _, hooked_present1 as _)? };
        hook.present1 = Some(present1_hook);
    }

    Ok(())
}

pub fn cleanup_hook() -> anyhow::Result<()> {
    let mut hook = HOOK.write();

    hook.present.take();
    hook.present1.take();

    RENDERER.dx11.lock().take();

    Ok(())
}

/// Get pointer to IDXGISwapChain::Present and IDXGISwapChain1::Present1 by creating dummy swapchain
fn get_dxgi_addr(dummy_hwnd: HWND) -> anyhow::Result<(PresentFn, Option<Present1Fn>)> {
    let (present_addr, present1_addr) = unsafe {
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

        let present = Interface::vtable(&swapchain).Present;
        let present1 = swapchain
            .cast::<IDXGISwapChain1>()
            .ok()
            .map(|swapchain1| Interface::vtable(&swapchain1).Present1);
        (present, present1)
    };

    Ok((present_addr, present1_addr))
}
