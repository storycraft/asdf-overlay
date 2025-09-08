mod input;

use asdf_overlay_event::{
    OverlayEvent, WindowEvent,
    input::{InputEvent, Key, KeyInputState, KeyboardInput},
};
use asdf_overlay_hook::DetourHook;
use core::cell::Cell;
use once_cell::sync::OnceCell;
use scopeguard::defer;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT},
        UI::{
            Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK, MapVirtualKeyA},
            WindowsAndMessaging::{
                self as msg, DefWindowProcA, GA_ROOT, GetAncestor, MSG, PEEK_MESSAGE_REMOVE_TYPE,
                PM_REMOVE,
            },
        },
    },
    core::BOOL,
};

use crate::{
    backend::{Backends, WindowBackend},
    event_sink::OverlayEventSink,
};

#[cfg_attr(
    not(target_arch = "x86"),
    link(name = "user32.dll", kind = "raw-dylib", modifiers = "+verbatim")
)]
#[cfg_attr(
    target_arch = "x86",
    link(
        name = "user32.dll",
        kind = "raw-dylib",
        modifiers = "+verbatim",
        import_name_type = "undecorated"
    )
)]
unsafe extern "system" {
    fn GetMessageA(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL;
    fn GetMessageW(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL;

    fn PeekMessageA(
        lpmsg: *mut MSG,
        hwnd: HWND,
        wmsgfiltermin: u32,
        wmsgfiltermax: u32,
        wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
    ) -> BOOL;
    fn PeekMessageW(
        lpmsg: *mut MSG,
        hwnd: HWND,
        wmsgfiltermin: u32,
        wmsgfiltermax: u32,
        wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
    ) -> BOOL;

    fn DispatchMessageA(msg: *const MSG) -> LRESULT;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
}

struct Hook {
    get_message_a: DetourHook<GetMessageFn>,
    get_message_w: DetourHook<GetMessageFn>,

    peek_message_a: DetourHook<PeekMessageFn>,
    peek_message_w: DetourHook<PeekMessageFn>,

    dispatch_message_a: DetourHook<DispatchMessageFn>,
    dispatch_message_w: DetourHook<DispatchMessageFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type GetMessageFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL;
type PeekMessageFn =
    unsafe extern "system" fn(*mut MSG, HWND, u32, u32, PEEK_MESSAGE_REMOVE_TYPE) -> BOOL;
type DispatchMessageFn = unsafe extern "system" fn(*const MSG) -> LRESULT;

pub fn hook() -> anyhow::Result<()> {
    input::hook()?;

    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking GetMessageA");
        let get_message_a = DetourHook::attach(GetMessageA as _, hooked_get_message_a as _)?;

        debug!("hooking GetMessageW");
        let get_message_w = DetourHook::attach(GetMessageW as _, hooked_get_message_w as _)?;

        debug!("hooking PeekMessageA");
        let peek_message_a = DetourHook::attach(PeekMessageA as _, hooked_peek_message_a as _)?;

        debug!("hooking PeekMessageW");
        let peek_message_w = DetourHook::attach(PeekMessageW as _, hooked_peek_message_w as _)?;

        debug!("hooking DispatchMessageA");
        let dispatch_message_a =
            DetourHook::attach(DispatchMessageA as _, hooked_dispatch_message_a as _)?;

        debug!("hooking DispatchMessageW");
        let dispatch_message_w =
            DetourHook::attach(DispatchMessageW as _, hooked_dispatch_message_w as _)?;

        Ok::<_, anyhow::Error>(Hook {
            get_message_a,
            get_message_w,

            peek_message_a,
            peek_message_w,

            dispatch_message_a,
            dispatch_message_w,
        })
    })?;

    Ok(())
}

thread_local! {
    static MESSAGE_READING: Cell<bool> = const { Cell::new(false) };
}

#[inline]
fn message_reading() -> bool {
    MESSAGE_READING.get()
}

#[inline]
fn set_message_read<R>(f: impl FnOnce() -> R) -> R {
    let last = MESSAGE_READING.replace(true);
    defer!(MESSAGE_READING.set(last));
    f()
}

#[tracing::instrument]
extern "system" fn hooked_get_message_a(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageA called");
    set_message_read(|| unsafe {
        let ret =
            HOOK.wait().get_message_a.original_fn()(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax);
        on_message_read(&*lpmsg);
        ret
    })
}

#[tracing::instrument]
extern "system" fn hooked_get_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageW called");
    set_message_read(|| unsafe {
        let ret =
            HOOK.wait().get_message_w.original_fn()(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax);
        on_message_read(&*lpmsg);
        ret
    })
}

#[tracing::instrument]
extern "system" fn hooked_peek_message_a(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
    wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
) -> BOOL {
    trace!("PeekMessageA called");
    set_message_read(|| unsafe {
        let ret = HOOK.wait().peek_message_a.original_fn()(
            lpmsg,
            hwnd,
            wmsgfiltermin,
            wmsgfiltermax,
            wremovemsg,
        );
        if ret.as_bool() && wremovemsg.contains(PM_REMOVE) {
            on_message_read(&*lpmsg);
        }
        ret
    })
}

#[tracing::instrument]
extern "system" fn hooked_peek_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
    wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
) -> BOOL {
    trace!("PeekMessageW called");
    set_message_read(|| unsafe {
        let ret = HOOK.wait().peek_message_w.original_fn()(
            lpmsg,
            hwnd,
            wmsgfiltermin,
            wmsgfiltermax,
            wremovemsg,
        );
        if ret.as_bool() && wremovemsg.contains(PM_REMOVE) {
            on_message_read(&*lpmsg);
        }
        ret
    })
}

fn on_message_read(msg: &MSG) {
    if msg.hwnd.is_invalid() {
        return;
    }

    _ = Backends::with_backend(msg.hwnd.0 as _, |backend| {
        let mut proc_queue = backend.proc_queue.lock();
        if proc_queue.is_empty() {
            return;
        }

        for f in proc_queue.drain(..) {
            f(backend);
        }
    });
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

#[inline]
fn dispatch_message(msg: &MSG) -> Option<LRESULT> {
    if msg.hwnd.is_invalid() {
        return None;
    }

    // Keyboard messages only dispatched to focused window.
    // Listening on DispatchMessage hook allow to hook keyboard messages going to child window
    match msg.message {
        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            return with_root_backend(msg, |backend| {
                emit_key_input(backend, msg, KeyInputState::Pressed)
            })
            .flatten();
        }

        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            return with_root_backend(msg, |backend| {
                emit_key_input(backend, msg, KeyInputState::Released)
            })
            .flatten();
        }

        // unicode characters are handled in WM_IME_COMPOSITION
        msg::WM_CHAR | msg::WM_SYSCHAR => {
            return with_root_backend(msg, |backend| {
                let proc = backend.proc.lock();
                if !proc.listening_keyboard() {
                    return None;
                }

                if let Some(ch) = char::from_u32(msg.wParam.0 as _) {
                    OverlayEventSink::emit(keyboard_input(backend.id, KeyboardInput::Char(ch)));
                }

                if proc.input_blocking() {
                    Some(LRESULT(0))
                } else {
                    None
                }
            })
            .flatten();
        }

        _ => {}
    }

    None
}

#[inline]
fn with_root_backend<R>(msg: &MSG, f: impl FnOnce(&WindowBackend) -> R) -> Option<R> {
    let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
    if root_hwnd.is_invalid() {
        return None;
    }

    Backends::with_backend(root_hwnd.0 as _, f)
}

#[inline]
fn emit_key_input(backend: &WindowBackend, msg: &MSG, state: KeyInputState) -> Option<LRESULT> {
    let proc = backend.proc.lock();
    if !proc.listening_keyboard() {
        return None;
    }

    if let Some(key) = to_key(msg.lParam) {
        OverlayEventSink::emit(keyboard_input(
            backend.id,
            KeyboardInput::Key { key, state },
        ));
    }

    if proc.input_blocking() {
        drop(proc);
        Some(unsafe { DefWindowProcA(HWND(backend.id as _), msg.message, msg.wParam, msg.lParam) })
    } else {
        None
    }
}

#[inline(always)]
fn keyboard_input(id: u32, input: KeyboardInput) -> OverlayEvent {
    OverlayEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Keyboard(input)),
    }
}

#[inline]
fn to_key(lparam: LPARAM) -> Option<Key> {
    let [_, _, code, flags] = bytemuck::cast::<_, [u8; 4]>(lparam.0 as u32);
    Key::new(
        unsafe { MapVirtualKeyA(code as u32, MAPVK_VSC_TO_VK) as u8 },
        flags & 0x01 == 0x01,
    )
}
