use core::ffi::c_void;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{Level, debug};
use windows::{
    Win32::{
        Foundation::{POINT, RECT},
        UI::Input::{HRAWINPUT, RAW_INPUT_DATA_COMMAND_FLAGS, RAWINPUT},
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

pub struct Hook {
    pub clip_cursor: DetourHook<ClipCursorFn>,
    pub set_cursor_pos: DetourHook<SetCursorPosFn>,

    pub get_clip_cursor: DetourHook<GetClipCursorFn>,
    pub get_cursor_pos: DetourHook<GetCursorPos>,
    pub get_physical_cursor_pos: DetourHook<GetPhysicalCursorPos>,
    pub get_async_key_state: DetourHook<GetAsyncKeyStateFn>,
    pub get_key_state: DetourHook<GetKeyStateFn>,
    pub get_keyboard_state: DetourHook<GetKeyboardStateFn>,
    pub get_raw_input_buffer: DetourHook<GetRawInputBufferFn>,
}
pub static HOOK: OnceCell<Hook> = OnceCell::new();

type ClipCursorFn = unsafe extern "system" fn(*const RECT) -> BOOL;
type SetCursorPosFn = unsafe extern "system" fn(i32, i32) -> BOOL;

type GetClipCursorFn = unsafe extern "system" fn(*mut RECT) -> BOOL;
type GetCursorPos = unsafe extern "system" fn(*mut POINT) -> BOOL;
type GetPhysicalCursorPos = unsafe extern "system" fn(*mut POINT) -> BOOL;
type GetAsyncKeyStateFn = unsafe extern "system" fn(i32) -> i16;
type GetKeyStateFn = unsafe extern "system" fn(i32) -> i16;
type GetKeyboardStateFn = unsafe extern "system" fn(*mut u8) -> BOOL;
type GetRawInputBufferFn = unsafe extern "system" fn(*mut RAWINPUT, *mut u32, u32) -> u32;

pub fn install() -> anyhow::Result<()> {
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
            get_raw_input_buffer,
        })
    })?;

    Ok(())
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_clip_cursor(lprect: *const RECT) -> BOOL {
    let mut lock = Backends::get().blocking_state.write();
    let Some(state) = lock.as_mut() else {
        drop(lock);
        return unsafe { HOOK.wait().clip_cursor.original_fn()(lprect) };
    };

    state.clip_cursor = unsafe { lprect.as_ref() }.copied();
    BOOL(1)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_set_cursor_pos(x: i32, y: i32) -> BOOL {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().set_cursor_pos.original_fn()(x, y) };
    }

    BOOL(1)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_clip_cursor(lprect: *mut RECT) -> BOOL {
    let lock = Backends::get().blocking_state.read();
    let Some(clip_cursor) = lock.as_ref().and_then(|state| state.clip_cursor) else {
        drop(lock);
        return unsafe { HOOK.wait().get_clip_cursor.original_fn()(lprect) };
    };

    unsafe { lprect.write(clip_cursor) };
    BOOL(1)
}

#[tracing::instrument(level = Level::TRACE)]
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

#[tracing::instrument(level = Level::TRACE)]
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

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_async_key_state(vkey: i32) -> i16 {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_async_key_state.original_fn()(vkey) };
    }

    0
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_key_state(vkey: i32) -> i16 {
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_key_state.original_fn()(vkey) };
    }

    0
}

#[tracing::instrument(level = Level::TRACE)]
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

#[tracing::instrument(level = Level::TRACE)]
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
