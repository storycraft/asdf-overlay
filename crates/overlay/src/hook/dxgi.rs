use core::{ffi::c_void, ptr};

use anyhow::{Context, bail};
use parking_lot::Mutex;
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
                CreateDXGIFactory1, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC,
                DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory1,
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
    let original = get_dxgi_present_addr()?;
    let hook = unsafe { Hook::new(original, hooked)? };
    unsafe { hook.enable()? };
    *HOOK.lock() = Some(hook);

    Ok(())
}

pub fn cleanup_hook() -> anyhow::Result<()> {
    let Some(hook) = HOOK.lock().take() else {
        return Ok(());
    };

    unsafe { hook.disable()? };

    Ok(())
}

type PresentFn = unsafe extern "system" fn(*mut c_void, u32, DXGI_PRESENT) -> HRESULT;

type Hook = GenericDetour<PresentFn>;
static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

unsafe extern "system" fn hooked(
    this: *mut c_void,
    sync_interval: u32,
    flags: DXGI_PRESENT,
) -> HRESULT {
    let Some(ref mut hook) = *HOOK.lock() else {
        return HRESULT(0);
    };

    println!("Present called");

    unsafe { hook.call(this, sync_interval, flags) }
}

/// Get pointer to IDXGISwapChain::Present by creating dummy swapchain
fn get_dxgi_present_addr() -> anyhow::Result<PresentFn> {
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

    let present_addr = unsafe {
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

        Interface::vtable(&swapchain.context("SwapChain creation failed")?).Present
    };

    Ok(present_addr)
}
