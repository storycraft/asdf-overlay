mod input;
pub use input::util;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, trace};
use windows::Win32::{Foundation::LRESULT, UI::WindowsAndMessaging::MSG};

use crate::backend::proc::dispatch_message;

#[link(name = "user32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn DispatchMessageA(msg: *const MSG) -> LRESULT;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
}

struct Hook {
    dispatch_message_a: DetourHook<DispatchMessageFn>,
    dispatch_message_w: DetourHook<DispatchMessageFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type DispatchMessageFn = unsafe extern "system" fn(*const MSG) -> LRESULT;

pub fn hook() -> anyhow::Result<()> {
    input::hook()?;

    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking DispatchMessageA");
        let dispatch_message_a =
            DetourHook::attach(DispatchMessageA as _, hooked_dispatch_message_a as _)?;

        debug!("hooking DispatchMessageW");
        let dispatch_message_w =
            DetourHook::attach(DispatchMessageW as _, hooked_dispatch_message_w as _)?;

        Ok::<_, anyhow::Error>(Hook {
            dispatch_message_a,
            dispatch_message_w,
        })
    })?;

    Ok(())
}

#[tracing::instrument]
extern "system" fn hooked_dispatch_message_a(msg: *const MSG) -> LRESULT {
    trace!("DispatchMessageA called");

    if let Some(ret) = dispatch_message(unsafe { &*msg }) {
        return ret;
    }

    unsafe { HOOK.wait().dispatch_message_a.original_fn()(msg) }
}

#[tracing::instrument]
extern "system" fn hooked_dispatch_message_w(msg: *const MSG) -> LRESULT {
    trace!("DispatchMessageW called");

    if let Some(ret) = dispatch_message(unsafe { &*msg }) {
        return ret;
    }

    unsafe { HOOK.wait().dispatch_message_w.original_fn()(msg) }
}
