use core::mem;

use once_cell::sync::OnceCell;
use tracing::trace;
use windows::Win32::{Foundation::LRESULT, UI::WindowsAndMessaging::MSG};

use crate::backend::{Backends, WindowBackend};

use super::DetourHook;

static HOOK: OnceCell<(DetourHook, DetourHook)> = OnceCell::new();

windows_link::link!("user32.dll" "system" fn DispatchMessageW(lpmsg : *const MSG) -> LRESULT);
windows_link::link!("user32.dll" "system" fn DispatchMessageA(lpmsg : *const MSG) -> LRESULT);

#[tracing::instrument]
pub fn hook() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| {
        Ok::<_, anyhow::Error>((
            unsafe { DetourHook::attach(DispatchMessageA as _, hooked_dispatch_message_a as _)? },
            unsafe { DetourHook::attach(DispatchMessageW as _, hooked_dispatch_message_w as _)? },
        ))
    })?;

    Ok(())
}

#[tracing::instrument(skip(backend))]
fn handle_message(backend: &WindowBackend, msg: &MSG) -> Option<LRESULT> {
    trace!("DispatchMessage called");
    None
}

unsafe extern "system" fn hooked_dispatch_message_a(msg: *const MSG) -> LRESULT {
    let msg = unsafe { &*msg };
    if !msg.hwnd.is_invalid() {
        if let Some(filtered) =
            Backends::with_backend(msg.hwnd, |backend| handle_message(backend, msg)).flatten()
        {
            return filtered;
        }
    }

    let (ref hook, _) = *HOOK.get().unwrap();
    unsafe { mem::transmute::<*const (), fn(*const MSG) -> LRESULT>(hook.original_fn())(msg) }
}

unsafe extern "system" fn hooked_dispatch_message_w(msg: *const MSG) -> LRESULT {
    let msg = unsafe { &*msg };

    if !msg.hwnd.is_invalid() {
        if let Some(filtered) =
            Backends::with_backend(msg.hwnd, |backend| handle_message(backend, msg)).flatten()
        {
            return filtered;
        }
    }

    let (_, ref hook) = *HOOK.get().unwrap();
    unsafe { mem::transmute::<*const (), fn(*const MSG) -> LRESULT>(hook.original_fn())(msg) }
}
