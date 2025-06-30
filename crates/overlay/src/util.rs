use core::mem::{self, ManuallyDrop};
use std::ffi::CString;

use anyhow::bail;
use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            CS_OWNDC, CreateWindowExA, DefWindowProcW, DestroyWindow, GetClientRect,
            RegisterClassA, UnregisterClassA, WINDOW_EX_STYLE, WNDCLASSA, WS_POPUP,
        },
    },
    core::{Interface, PCSTR, s},
};

use crate::INSTANCE;

// Cloning COM objects for ManuallyDrop<Option<T>> never decrease ref count and leak wtf
// as per: https://github.com/microsoft/windows-rs/blob/83d4e0b4d49d004f52523614f292bc1526142052/crates/samples/windows/direct3d12/src/main.rs#L493
pub unsafe fn wrap_com_manually_drop<T: Interface>(inf: &T) -> ManuallyDrop<Option<T>> {
    unsafe { mem::transmute_copy(inf) }
}

pub fn get_client_size(win: HWND) -> anyhow::Result<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(win, &mut rect)? };

    Ok((rect.right as u32, rect.bottom as u32))
}

pub fn with_dummy_hwnd<R>(f: impl FnOnce(HWND) -> R) -> anyhow::Result<R> {
    extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    unsafe {
        let instance = INSTANCE.get().copied().unwrap_or_default();
        let class_name =
            CString::new(format!("asdf-overlay-{} dummy window class", instance)).unwrap();
        let hinstance = HINSTANCE(instance as _);

        if RegisterClassA(&WNDCLASSA {
            style: CS_OWNDC,
            hInstance: hinstance,
            lpszClassName: PCSTR(class_name.as_ptr() as _),
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        }) == 0
        {
            bail!("RegisterClassA call failed");
        }
        defer!({
            _ = UnregisterClassA(PCSTR(class_name.as_ptr() as _), Some(hinstance));
        });

        let hwnd = CreateWindowExA(
            WINDOW_EX_STYLE(0),
            PCSTR(class_name.as_ptr() as _),
            s!("asdf-overlay dummy window"),
            WS_POPUP,
            0,
            0,
            2,
            2,
            None,
            None,
            Some(hinstance),
            None,
        )?;
        defer!({
            _ = DestroyWindow(hwnd);
        });

        Ok(f(hwnd))
    }
}
