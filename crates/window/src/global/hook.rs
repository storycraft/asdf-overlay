use core::ffi::c_void;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::debug;
use windows::{
    Win32::{
        Foundation::{POINT, RECT},
        UI::Input::{
            HRAWINPUT, RAW_INPUT_DATA_COMMAND_FLAGS, RAWINPUT, RAWINPUTHEADER, RID_HEADER,
            RID_INPUT,
        },
    },
    core::BOOL,
};

use crate::Backends;

windows::core::link!("user32.dll" "system" fn ClipCursor(lprect: *const RECT) -> BOOL);
windows::core::link!("user32.dll" "system" fn SetCursorPos(x: i32, y: i32) -> BOOL);

windows::core::link!("user32.dll" "system" fn GetClipCursor(lprect: *mut RECT) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetCursorPos(lppoint: *mut POINT) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetPhysicalCursorPos(lppoint: *mut POINT) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetKeyboardState(buf: *mut u8) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetKeyState(vkey: i32) -> i16);
windows::core::link!("user32.dll" "system" fn GetAsyncKeyState(vkey: i32) -> i16);
windows::core::link!(
    "user32.dll" "system"
    fn GetRawInputData(
        hrawinput: HRAWINPUT,
        uicommand: RAW_INPUT_DATA_COMMAND_FLAGS,
        pdata: *mut c_void,
        pcbsize: *mut u32,
        cbsizeheader: u32,
    ) -> u32
);
windows::core::link!("user32.dll" "system" fn GetRawInputBuffer(pdata: *mut RAWINPUT, pcbsize: *mut u32, cbsizeheader: u32) -> u32);

pub(crate) struct Hook {
    pub(crate) clip_cursor: DetourHook<ClipCursorFn>,
    pub(crate) set_cursor_pos: DetourHook<SetCursorPosFn>,

    pub(crate) get_clip_cursor: DetourHook<GetClipCursorFn>,
    pub(crate) get_cursor_pos: DetourHook<GetCursorPos>,
    pub(crate) get_physical_cursor_pos: DetourHook<GetPhysicalCursorPos>,
    pub(crate) get_async_key_state: DetourHook<GetAsyncKeyStateFn>,
    pub(crate) get_key_state: DetourHook<GetKeyStateFn>,
    pub(crate) get_keyboard_state: DetourHook<GetKeyboardStateFn>,
    pub(crate) get_raw_input_data: DetourHook<GetRawInputDataFn>,
    pub(crate) get_raw_input_buffer: DetourHook<GetRawInputBufferFn>,
}
pub(crate) static HOOK: OnceCell<Hook> = OnceCell::new();

type ClipCursorFn = unsafe extern "system" fn(*const RECT) -> BOOL;
type SetCursorPosFn = unsafe extern "system" fn(i32, i32) -> BOOL;

type GetClipCursorFn = unsafe extern "system" fn(*mut RECT) -> BOOL;
type GetCursorPos = unsafe extern "system" fn(*mut POINT) -> BOOL;
type GetPhysicalCursorPos = unsafe extern "system" fn(*mut POINT) -> BOOL;
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

pub(crate) fn install() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking ClipCursor");
        let clip_cursor = DetourHook::attach(ClipCursor as _, hooked_clip_cursor as _)?;

        debug!("hooking SetCursorPos");
        let set_cursor_pos = DetourHook::attach(SetCursorPos as _, hooked_set_cursor_pos as _)?;

        debug!("hooking GetClipCursor");
        let get_clip_cursor = DetourHook::attach(GetClipCursor as _, hooked_get_clip_cursor as _)?;

        debug!("hooking GetCursorPos");
        let get_cursor_pos = DetourHook::attach(GetCursorPos as _, hooked_get_cursor_pos as _)?;

        debug!("hooking GetPhysicalCursorPos");
        let get_physical_cursor_pos = DetourHook::attach(
            GetPhysicalCursorPos as _,
            hooked_get_physical_cursor_pos as _,
        )?;

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
            clip_cursor,
            set_cursor_pos,

            get_clip_cursor,
            get_cursor_pos,
            get_physical_cursor_pos,
            get_async_key_state,
            get_key_state,
            get_keyboard_state,
            get_raw_input_data,
            get_raw_input_buffer,
        })
    })?;

    Ok(())
}

#[tracing::instrument]
extern "system" fn hooked_clip_cursor(lprect: *const RECT) -> BOOL {
    let mut lock = Backends::get().blocking_state.write();
    let Some(state) = lock.as_mut() else {
        drop(lock);
        return unsafe { HOOK.wait().clip_cursor.original_fn()(lprect) };
    };

    state.clip_cursor = unsafe { lprect.as_ref() }.copied();
    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_set_cursor_pos(x: i32, y: i32) -> BOOL {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().set_cursor_pos.original_fn()(x, y) };
    }

    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_get_clip_cursor(lprect: *mut RECT) -> BOOL {
    let lock = Backends::get().blocking_state.read();
    let Some(clip_cursor) = lock.as_ref().and_then(|state| state.clip_cursor) else {
        drop(lock);
        return unsafe { HOOK.wait().get_clip_cursor.original_fn()(lprect) };
    };

    unsafe { lprect.write(clip_cursor) };
    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_get_cursor_pos(lppoint: *mut POINT) -> BOOL {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_cursor_pos.original_fn()(lppoint) };
    }

    // Return a fixed position instead of the real cursor position to prevent games from tracking mouse movement
    unsafe {
        lppoint.write(POINT { x: 0, y: 0 });
    }
    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_get_physical_cursor_pos(lppoint: *mut POINT) -> BOOL {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_physical_cursor_pos.original_fn()(lppoint) };
    }

    // Return a fixed position instead of the real cursor position to prevent games from tracking mouse movement
    unsafe {
        lppoint.write(POINT { x: 0, y: 0 });
    }
    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_get_async_key_state(vkey: i32) -> i16 {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_async_key_state.original_fn()(vkey) };
    }

    0
}

#[tracing::instrument]
extern "system" fn hooked_get_key_state(vkey: i32) -> i16 {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_key_state.original_fn()(vkey) };
    }

    0
}

#[tracing::instrument]
extern "system" fn hooked_get_keyboard_state(buf: *mut u8) -> BOOL {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_keyboard_state.original_fn()(buf) };
    }

    // buf is 256 bytes array according to doc.
    unsafe {
        buf.write_bytes(0u8, 256);
    };
    BOOL(1)
}

#[tracing::instrument]
extern "system" fn hooked_get_raw_input_data(
    hrawinput: HRAWINPUT,
    uicommand: RAW_INPUT_DATA_COMMAND_FLAGS,
    pdata: *mut c_void,
    pcbsize: *mut u32,
    cbsizeheader: u32,
) -> u32 {
    if !Backends::get().input_blocked() {
        return unsafe {
            HOOK.wait().get_raw_input_data.original_fn()(
                hrawinput,
                uicommand,
                pdata,
                pcbsize,
                cbsizeheader,
            )
        };
    }

    // Determine the expected data size based on the command, matching the
    // real Win32 API behaviour so callers (e.g. Godot 4) that validate the
    // returned size against the queried size do not crash.
    let data_size: u32 = match uicommand {
        RID_HEADER => core::mem::size_of::<RAWINPUTHEADER>() as u32,
        RID_INPUT => core::mem::size_of::<RAWINPUT>() as u32,
        _ => 0,
    };

    if !pcbsize.is_null() {
        unsafe { pcbsize.write(data_size) };
    }

    // Size query: return 0 (success) after writing the required buffer size.
    if pdata.is_null() {
        return 0;
    }

    // Data query: write a dummy HID struct and return the number of bytes
    // written, exactly as the real API would on success.
    // Use RIM_TYPEHID so games treat this as an unknown HID device and
    // ignore the empty payload instead of interpreting zeroed fields as
    // mouse/keyboard input.

    let hid_header = RAWINPUTHEADER {
        dwType: 2, // RIM_TYPEHID
        dwSize: data_size,
        ..Default::default()
    };

    match uicommand {
        RID_HEADER => unsafe {
            pdata.cast::<RAWINPUTHEADER>().write(hid_header);
        },
        RID_INPUT => unsafe {
            pdata.cast::<RAWINPUT>().write(RAWINPUT {
                header: hid_header,
                ..Default::default()
            });
        },
        _ => {}
    }

    data_size
}

#[tracing::instrument]
extern "system" fn hooked_get_raw_input_buffer(
    pdata: *mut RAWINPUT,
    pcbsize: *mut u32,
    cbsizeheader: u32,
) -> u32 {
    if !Backends::get().input_blocked() {
        return unsafe {
            HOOK.wait().get_raw_input_buffer.original_fn()(pdata, pcbsize, cbsizeheader)
        };
    }

    unsafe { *pcbsize = 0 };
    0
}
