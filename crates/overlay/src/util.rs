use anyhow::bail;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleA,
        UI::WindowsAndMessaging::{
            CS_OWNDC, CreateWindowExA, DefWindowProcW, DestroyWindow, GetClientRect,
            RegisterClassA, UnregisterClassA, WINDOW_EX_STYLE, WNDCLASSA, WS_POPUP,
        },
    },
    core::{PCSTR, s},
};

pub fn get_client_size(win: HWND) -> anyhow::Result<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(win, &mut rect)? };

    Ok((
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ))
}

pub fn with_dummy_hwnd<R>(f: impl FnOnce(HWND) -> R) -> anyhow::Result<R> {
    const CLASS_NAME: PCSTR = s!("asdf-overlay dummy window class");
    const NAME: PCSTR = s!("asdf-overlay dummy window");

    extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    unsafe {
        let h_instance = GetModuleHandleA(None)?.into();

        if RegisterClassA(&WNDCLASSA {
            style: CS_OWNDC,
            hInstance: h_instance,
            lpszClassName: CLASS_NAME,
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        }) == 0
        {
            bail!("RegisterClassA call failed");
        }
        defer!({
            _ = UnregisterClassA(CLASS_NAME, Some(h_instance));
        });

        let hwnd = CreateWindowExA(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            NAME,
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(h_instance),
            None,
        )?;
        defer!({
            _ = DestroyWindow(hwnd);
        });

        Ok(f(hwnd))
    }
}
