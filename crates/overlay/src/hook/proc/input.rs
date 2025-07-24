use core::ffi::c_void;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::debug;
use windows::{
    Win32::{
        Foundation::POINT,
        UI::{
            Input::{
                HRAWINPUT, KeyboardAndMouse::GetActiveWindow, RAW_INPUT_DATA_COMMAND_FLAGS,
                RAWINPUT,
            },
            WindowsAndMessaging::GetForegroundWindow,
        },
    },
    core::BOOL,
};

use crate::backend::Backends;

#[link(name = "user32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn GetKeyboardState(buf: *mut u8) -> BOOL;
    fn GetKeyState(vkey: i32) -> i16;
    fn GetAsyncKeyState(vkey: i32) -> i16;
    fn GetCursorPos(lppoint: *mut POINT) -> BOOL;
    fn GetRawInputData(
        hrawinput: HRAWINPUT,
        uicommand: RAW_INPUT_DATA_COMMAND_FLAGS,
        pdata: *mut c_void,
        pcbsize: *mut u32,
        cbsizeheader: u32,
    ) -> u32;
    fn GetRawInputBuffer(pdata: *mut RAWINPUT, pcbsize: *mut u32, cbsizeheader: u32) -> u32;

}

struct Hook {
    get_cursor_pos: DetourHook<GetCursorPos>,
    get_async_key_state: DetourHook<GetAsyncKeyStateFn>,
    get_key_state: DetourHook<GetKeyStateFn>,
    get_keyboard_state: DetourHook<GetKeyboardStateFn>,
    get_raw_input_data: DetourHook<GetRawInputDataFn>,
    get_raw_input_buffer: DetourHook<GetRawInputBufferFn>,
}
static HOOK: OnceCell<Hook> = OnceCell::new();

type GetCursorPos = unsafe extern "system" fn(*mut POINT) -> BOOL;
type GetAsyncKeyStateFn = unsafe extern "system" fn(i32) -> i16;
type GetKeyStateFn = unsafe extern "system" fn(i32) -> i16;
type GetKeyboardStateFn = unsafe extern "system" fn(*mut u8) -> BOOL;
type GetRawInputDataFn = unsafe extern "system" fn(
    HRAWINPUT,
    RAW_INPUT_DATA_COMMAND_FLAGS,
    *mut c_void,
    *mut u32,
    u32,
) -> u32;
type GetRawInputBufferFn = unsafe extern "system" fn(*mut RAWINPUT, *mut u32, u32) -> u32;

pub fn hook() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking GetCursorPos");
        let get_cursor_pos = DetourHook::attach(GetCursorPos as _, hooked_get_cursor_pos as _)?;

        debug!("hooking GetAsyncKeyState");
        let get_async_key_state =
            DetourHook::attach(GetAsyncKeyState as _, hooked_get_async_key_state as _)?;

        debug!("hooking GetKeyState");
        let get_key_state = DetourHook::attach(GetKeyState as _, hooked_get_key_state as _)?;

        debug!("hooking GetKeyboardState");
        let get_keyboard_state =
            DetourHook::attach(GetKeyboardState as _, hooked_get_keyboard_state as _)?;

        debug!("hooking GetRawInputData");
        let get_raw_input_data =
            DetourHook::attach(GetRawInputData as _, hooked_get_raw_input_data as _)?;

        debug!("hooking GetRawInputBuffer");
        let get_raw_input_buffer =
            DetourHook::attach(GetRawInputBuffer as _, hooked_get_raw_input_buffer as _)?;

        Ok::<_, anyhow::Error>(Hook {
            get_cursor_pos,
            get_async_key_state,
            get_key_state,
            get_keyboard_state,
            get_raw_input_data,
            get_raw_input_buffer,
        })
    })?;

    Ok(())
}

#[inline]
fn active_hwnd_input_blocked() -> bool {
    let hwnd = unsafe { GetActiveWindow() };

    !hwnd.is_invalid()
        && Backends::with_backend(hwnd, |backend| backend.input_blocking()).unwrap_or(false)
}

#[inline]
fn foreground_hwnd_input_blocked() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };

    !hwnd.is_invalid()
        && Backends::with_backend(hwnd, |backend| backend.input_blocking()).unwrap_or(false)
}

#[tracing::instrument]
extern "system" fn hooked_get_cursor_pos(lppoint: *mut POINT) -> BOOL {
    if foreground_hwnd_input_blocked() {
        return BOOL(1);
    }

    unsafe { HOOK.wait().get_cursor_pos.original_fn()(lppoint) }
}

#[tracing::instrument]
extern "system" fn hooked_get_async_key_state(vkey: i32) -> i16 {
    if foreground_hwnd_input_blocked() {
        return 0;
    }

    unsafe { HOOK.wait().get_async_key_state.original_fn()(vkey) }
}

#[tracing::instrument]
extern "system" fn hooked_get_key_state(vkey: i32) -> i16 {
    if active_hwnd_input_blocked() {
        return 0;
    }

    unsafe { HOOK.wait().get_key_state.original_fn()(vkey) }
}

#[tracing::instrument]
extern "system" fn hooked_get_keyboard_state(buf: *mut u8) -> BOOL {
    if active_hwnd_input_blocked() {
        return BOOL(1);
    }

    unsafe { HOOK.wait().get_keyboard_state.original_fn()(buf) }
}

#[tracing::instrument]
extern "system" fn hooked_get_raw_input_data(
    hrawinput: HRAWINPUT,
    uicommand: RAW_INPUT_DATA_COMMAND_FLAGS,
    pdata: *mut c_void,
    pcbsize: *mut u32,
    cbsizeheader: u32,
) -> u32 {
    if active_hwnd_input_blocked() {
        return 0;
    }

    unsafe {
        HOOK.wait().get_raw_input_data.original_fn()(
            hrawinput,
            uicommand,
            pdata,
            pcbsize,
            cbsizeheader,
        )
    }
}

#[tracing::instrument]
extern "system" fn hooked_get_raw_input_buffer(
    pdata: *mut RAWINPUT,
    pcbsize: *mut u32,
    cbsizeheader: u32,
) -> u32 {
    if active_hwnd_input_blocked() {
        return 0;
    }

    unsafe { HOOK.wait().get_raw_input_buffer.original_fn()(pdata, pcbsize, cbsizeheader) }
}
