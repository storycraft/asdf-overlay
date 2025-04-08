use core::{ffi::c_void, mem, ptr};

use anyhow::{Context, bail};
use parking_lot::{Mutex, RwLock};
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM},
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
                DXGI_SWAP_CHAIN_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory1,
                IDXGISwapChain, IDXGISwapChain1,
            },
        },
        System::LibraryLoader::GetModuleHandleA,
        UI::WindowsAndMessaging::{
            CS_OWNDC, CreateWindowExA, DefWindowProcW, DestroyWindow, GetClientRect,
            RegisterClassA, UnregisterClassA, WINDOW_EX_STYLE, WNDCLASSA, WS_POPUP,
        },
    },
    core::{BOOL, HRESULT, IUnknown, Interface, s},
};

use crate::renderer::dx11::Dx11Renderer;

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

    let test = flags & DXGI_PRESENT_TEST != DXGI_PRESENT(0);
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

    let test = flags & DXGI_PRESENT_TEST != DXGI_PRESENT(0);
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

    let size = {
        let Ok(desc) = (unsafe { swapchain.GetDesc() }) else {
            return;
        };

        let mut rect = RECT::default();
        unsafe { GetClientRect(desc.OutputWindow, &mut rect).unwrap() };

        (
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        )
    };

    if let Some(_) = device.cast::<ID3D12Device>().ok() {
    } else if let Some(device) = device.cast::<ID3D11Device>().ok() {
        let mut renderer = RENDERER.dx11.lock();
        let renderer = renderer
            .get_or_insert_with(|| Dx11Renderer::new(&device).expect("renderer creation failed"));

        _ = renderer.draw(&device, swapchain, size);
    } else if let Some(_) = device.cast::<ID3D10Device>().ok() {
    }
}

pub fn hook() -> anyhow::Result<()> {
    let (present, present1) = get_dxgi_addr()?;
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
fn get_dxgi_addr() -> anyhow::Result<(PresentFn, Option<Present1Fn>)> {
    extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let dxgi_dll = s!("dxgi.dll");
    unsafe { GetModuleHandleA(dxgi_dll)? };

    let class_name = s!("dummy window class");
    let name = s!("dummy window");

    let (present_addr, present1_addr) = unsafe {
        let h_instance = GetModuleHandleA(None)?.into();

        if RegisterClassA(&WNDCLASSA {
            style: CS_OWNDC,
            hInstance: h_instance,
            lpszClassName: class_name,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        }) == 0
        {
            bail!("failed to register window class");
        }
        defer!({
            let _ = UnregisterClassA(class_name, Some(h_instance));
        });

        let hwnd = CreateWindowExA(
            WINDOW_EX_STYLE(0),
            class_name,
            name,
            WS_POPUP,
            0,
            0,
            2,
            2,
            None,
            None,
            Some(h_instance),
            None,
        )?;
        defer!({
            let _ = DestroyWindow(hwnd);
        });

        let factory = CreateDXGIFactory1::<IDXGIFactory1>()?;
        let adapter = factory.EnumAdapters1(0)?;

        let desc = DXGI_SWAP_CHAIN_DESC {
            BufferCount: 2,
            BufferDesc: DXGI_MODE_DESC {
                Width: 2,
                Height: 2,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                ..Default::default()
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            OutputWindow: hwnd,
            Windowed: BOOL(1),
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
