use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::LRESULT,
        UI::WindowsAndMessaging::{GetForegroundWindow, MSG},
    },
    core::BOOL,
};

use crate::backend::{Backends, proc::dispatch_message};

#[link(name = "user32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn DispatchMessageA(msg: *const MSG) -> LRESULT;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn GetKeyboardState(buf: *mut u8) -> BOOL;
}

struct Hook {
    dispatch_message_a: DetourHook<DispatchMessageFn>,
    dispatch_message_w: DetourHook<DispatchMessageFn>,
    get_keyboard_state: DetourHook<GetKeyboardStateFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type DispatchMessageFn = unsafe extern "system" fn(*const MSG) -> LRESULT;
type GetKeyboardStateFn = unsafe extern "system" fn(*mut u8) -> BOOL;

pub fn hook() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking DispatchMessageA");
        let dispatch_message_a =
            DetourHook::attach(DispatchMessageA as _, hooked_dispatch_message_a as _)?;

        debug!("hooking DispatchMessageW");
        let dispatch_message_w =
            DetourHook::attach(DispatchMessageW as _, hooked_dispatch_message_w as _)?;

        debug!("hooking GetKeyboardState");
        let get_keyboard_state =
            DetourHook::attach(GetKeyboardState as _, hooked_get_keyboard_state as _)?;

        Ok::<_, anyhow::Error>(Hook {
            dispatch_message_a,
            dispatch_message_w,
            get_keyboard_state,
        })
    })?;

    Ok(())
}

#[tracing::instrument]
extern "system" fn hooked_get_keyboard_state(buf: *mut u8) -> BOOL {
    let hwnd = unsafe { GetForegroundWindow() };
    if !hwnd.is_invalid()
        && Backends::with_backend(hwnd, |backend| backend.input_blocking()).unwrap_or(false)
    {
        return BOOL(1);
    }

    let hook = HOOK.get().unwrap();
    unsafe { hook.get_keyboard_state.original_fn()(buf) }
}

#[tracing::instrument]
extern "system" fn hooked_dispatch_message_a(msg: *const MSG) -> LRESULT {
    trace!("DispatchMessageA called");

    if let Some(ret) = dispatch_message(unsafe { &*msg }) {
        return ret;
    }

    let hook = HOOK.get().unwrap();
    unsafe { hook.dispatch_message_a.original_fn()(msg) }
}

#[tracing::instrument]
extern "system" fn hooked_dispatch_message_w(msg: *const MSG) -> LRESULT {
    trace!("DispatchMessageW called");

    if let Some(ret) = dispatch_message(unsafe { &*msg }) {
        return ret;
    }

    let hook = HOOK.get().unwrap();
    unsafe { hook.dispatch_message_w.original_fn()(msg) }
}
