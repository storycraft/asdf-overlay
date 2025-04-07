use core::{ffi::c_void, ptr};

use anyhow::{Context, bail};
use parking_lot::RwLock;
use retour::GenericDetour;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct3D10::{
                D3D10_DRIVER_TYPE_HARDWARE, D3D10_SDK_VERSION, D3D10CreateDeviceAndSwapChain,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC},
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_PRESENT_TEST,
                DXGI_SWAP_CHAIN_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory1,
                IDXGISwapChain1,
            },
        },
        System::LibraryLoader::GetModuleHandleA,
        UI::WindowsAndMessaging::{
            CS_OWNDC, CreateWindowExA, DefWindowProcW, DestroyWindow, RegisterClassA,
            UnregisterClassA, WINDOW_EX_STYLE, WNDCLASSA, WS_POPUP,
        },
    },
    core::{BOOL, HRESULT, Interface, s},
};

pub fn hook() -> anyhow::Result<()> {
    let (present, present1) = get_dxgi_addr()?;
    let mut hook = HOOK.write();

    let present_hook = unsafe { PresentHook::new(present, hooked_present)? };
    unsafe { present_hook.enable()? };
    hook.present = Some(present_hook);

    if let Some(present1) = present1 {
        let present1_hook = unsafe { Present1Hook::new(present1, hooked_present1)? };
        unsafe { present1_hook.enable()? };
        hook.present1 = Some(present1_hook);
    }

    Ok(())
}

pub fn cleanup_hook() -> anyhow::Result<()> {
    let mut hook = HOOK.write();

    if let Some(present_hook) = hook.present.take() {
        unsafe { present_hook.disable()? };
    };

    if let Some(present1_hook) = hook.present1.take() {
        unsafe { present1_hook.disable()? };
    };

    Ok(())
}

type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;
type Present1Fn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    DXGI_PRESENT,
    *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT;

type PresentHook = GenericDetour<PresentFn>;
type Present1Hook = GenericDetour<Present1Fn>;

struct Hook {
    present: Option<PresentHook>,
    present1: Option<Present1Hook>,
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
    let Some(ref hook) = HOOK.read().present else {
        return HRESULT(0);
    };

    if flags & DXGI_PRESENT_TEST != DXGI_PRESENT(0) {}

    unsafe { hook.call(this, sync_interval, flags) }
}

unsafe extern "system" fn hooked_present1(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
    present_params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    let Some(ref present) = HOOK.read().present1 else {
        return HRESULT(0);
    };

    unsafe { present.call(this, sync_interval, flags, present_params) }
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
