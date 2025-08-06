mod input;
use asdf_overlay_common::{
    event::{
        ClientEvent, WindowEvent,
        input::{InputEvent, InputState, KeyboardInput},
    },
    key::Key,
};
pub use input::util;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use scopeguard::defer;
use tracing::{debug, trace};
use utf16string::{LittleEndian, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Input::Ime::{GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext},
        WindowsAndMessaging::{self as msg, DefWindowProcA, GA_ROOT, GetAncestor, MSG},
    },
};

use crate::{
    app::OverlayIpc,
    backend::{Backends, WindowBackend},
};

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

#[inline]
fn dispatch_message(msg: &MSG) -> Option<LRESULT> {
    if msg.hwnd.is_invalid() {
        return None;
    }

    let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
    Backends::with_backend(root_hwnd, |backend| {
        if backend.proc.lock().listening_keyboard() {
            process_keyboard_listen(backend, msg)
        } else {
            None
        }
    })
    .flatten()
}

#[inline]
fn process_keyboard_listen(backend: &WindowBackend, msg: &MSG) -> Option<LRESULT> {
    #[inline]
    fn emit_key_input(backend: &WindowBackend, msg: &MSG, state: InputState) -> Option<LRESULT> {
        if let Some(key) = to_key(msg.wParam, msg.lParam) {
            OverlayIpc::emit_event(keyboard_input(
                backend.hwnd,
                KeyboardInput::Key { key, state },
            ));

            if backend.proc.lock().input_blocking() {
                return Some(unsafe {
                    DefWindowProcA(msg.hwnd, msg.message, msg.wParam, msg.lParam)
                });
            }
        }

        None
    }

    match msg.message {
        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            return emit_key_input(backend, msg, InputState::Pressed);
        }

        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            return emit_key_input(backend, msg, InputState::Released);
        }

        // unicode characters are handled in WM_IME_COMPOSITION
        msg::WM_CHAR | msg::WM_SYSCHAR => {
            if let Some(ch) = char::from_u32(msg.wParam.0 as _) {
                OverlayIpc::emit_event(keyboard_input(backend.hwnd, KeyboardInput::Char(ch)));
            }

            if backend.proc.lock().input_blocking() {
                return Some(LRESULT(0));
            }
        }
        msg::WM_IME_COMPOSITION if msg.lParam.0 as u32 == GCS_RESULTSTR.0 => {
            if let Some(str) = get_ime_string(HWND(backend.hwnd as _)) {
                for ch in str.chars() {
                    OverlayIpc::emit_event(keyboard_input(backend.hwnd, KeyboardInput::Char(ch)));
                }
            }

            if backend.proc.lock().input_blocking() {
                return Some(LRESULT(0));
            }
        }

        // ignore remaining keyboard inputs
        msg::WM_APPCOMMAND
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR => {
            if backend.proc.lock().input_blocking() {
                return Some(LRESULT(0));
            }
        }

        _ => {}
    }

    None
}

#[inline(always)]
fn keyboard_input(hwnd: u32, input: KeyboardInput) -> ClientEvent {
    ClientEvent::Window {
        hwnd,
        event: WindowEvent::Input(InputEvent::Keyboard(input)),
    }
}

#[inline]
fn to_key(wparam: WPARAM, lparam: LPARAM) -> Option<Key> {
    let [_, _, _, flags] = bytemuck::cast::<_, [u8; 4]>(lparam.0 as u32);
    Key::new(wparam.0 as _, flags & 0x01 == 0x01)
}

#[inline]
fn get_ime_string(hwnd: HWND) -> Option<WString<LittleEndian>> {
    let himc = unsafe { ImmGetContext(hwnd) };
    defer!(unsafe {
        _ = ImmReleaseContext(hwnd, himc);
    });

    let byte_size = unsafe { ImmGetCompositionStringW(himc, GCS_RESULTSTR, None, 0) };
    if byte_size >= 0 {
        let mut buf = vec![0_u8; byte_size as usize];

        unsafe {
            ImmGetCompositionStringW(
                himc,
                GCS_RESULTSTR,
                Some(buf.as_mut_ptr().cast()),
                buf.len() as _,
            )
        };

        WString::from_utf16le(buf).ok()
    } else {
        None
    }
}
