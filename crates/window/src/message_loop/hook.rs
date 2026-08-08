use asdf_overlay_event::{
    OverlayEvent, WindowEvent,
    input::{CursorAction, CursorInput, InputEvent, Key, KeyInputState, KeyboardInput, ScrollAxis},
};
use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK, MapVirtualKeyA},
            WindowsAndMessaging::{
                self as msg, CallWindowProcA, CallWindowProcW, GA_ROOT, GetAncestor, MSG,
                PEEK_MESSAGE_REMOVE_TYPE, PM_REMOVE, TranslateMessage,
            },
        },
    },
    core::BOOL,
};

use crate::{Backends, proc::WindowProcState};

windows::core::link!("user32.dll" "system" fn GetMessageA(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetMessageW(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL);

windows::core::link!("user32.dll" "system" fn PeekMessageA(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
    wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
) -> BOOL);
windows::core::link!("user32.dll" "system" fn PeekMessageW(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
    wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
) -> BOOL);
windows::core::link!("user32.dll" "system" fn DefWindowProcA(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT);
windows::core::link!("user32.dll" "system" fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT);

pub(crate) struct Hook {
    pub(crate) get_message_a: DetourHook<GetMessageFn>,
    pub(crate) get_message_w: DetourHook<GetMessageFn>,

    pub(crate) peek_message_a: DetourHook<PeekMessageFn>,
    pub(crate) peek_message_w: DetourHook<PeekMessageFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type GetMessageFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL;
type PeekMessageFn =
    unsafe extern "system" fn(*mut MSG, HWND, u32, u32, PEEK_MESSAGE_REMOVE_TYPE) -> BOOL;

pub(crate) fn install() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking GetMessageA");
        let get_message_a = DetourHook::attach(GetMessageA as _, hooked_get_message_a as _)?;

        debug!("hooking GetMessageW");
        let get_message_w = DetourHook::attach(GetMessageW as _, hooked_get_message_w as _)?;

        debug!("hooking PeekMessageA");
        let peek_message_a = DetourHook::attach(PeekMessageA as _, hooked_peek_message_a as _)?;

        debug!("hooking PeekMessageW");
        let peek_message_w = DetourHook::attach(PeekMessageW as _, hooked_peek_message_w as _)?;

        Ok::<_, anyhow::Error>(Hook {
            get_message_a,
            get_message_w,

            peek_message_a,
            peek_message_w,
        })
    })?;

    Ok(())
}

fn get_message<const UNICODE: bool>(
    msg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    let original_read = unsafe {
        if UNICODE {
            HOOK.wait().get_message_w.original_fn()(msg, hwnd, wmsgfiltermin, wmsgfiltermax)
        } else {
            HOOK.wait().get_message_a.original_fn()(msg, hwnd, wmsgfiltermin, wmsgfiltermax)
        }
    };

    // If there were error on GetMessageA/W.
    if original_read.0 == -1 {
        return original_read;
    }

    let msg = unsafe { &mut *msg };
    if read_message::<UNICODE>(msg) {
        msg.message = msg::WM_NULL;
    }
    original_read
}

fn peek_message<const UNICODE: bool>(
    msg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
    remove: PEEK_MESSAGE_REMOVE_TYPE,
) -> BOOL {
    let original_read = unsafe {
        if UNICODE {
            HOOK.wait().peek_message_a.original_fn()(
                msg,
                hwnd,
                wmsgfiltermin,
                wmsgfiltermax,
                remove,
            )
        } else {
            HOOK.wait().peek_message_w.original_fn()(
                msg,
                hwnd,
                wmsgfiltermin,
                wmsgfiltermax,
                remove,
            )
        }
    };

    if !original_read.as_bool() {
        return original_read;
    }

    let msg = unsafe { &mut *msg };
    if !remove.contains(PM_REMOVE) {
        // Only hide non removed messages without processing so apps cannot see them.
        if should_filter(msg) {
            msg.message = msg::WM_NULL;
        }
        return original_read;
    }

    if read_message::<UNICODE>(msg) {
        msg.message = msg::WM_NULL;
    }
    original_read
}

#[tracing::instrument]
extern "system" fn hooked_get_message_a(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageA called");
    get_message::<false>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax).into()
}

#[tracing::instrument]
extern "system" fn hooked_get_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageW called");
    get_message::<true>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax).into()
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
    peek_message::<false>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax, wremovemsg)
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
    peek_message::<true>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax, wremovemsg)
}

/// Process the message.
///
/// Returns true if the message should be filtered (not processed by the application).
fn read_message<const UNICODE: bool>(msg: &MSG) -> bool {
    let id = unsafe { GetCurrentThreadId() };

    if msg.message == msg::WM_QUIT {
        Backends::get().cleanup_message_loop(id);
        return false;
    }

    Backends::get().message_loop_state(id, |msg_loop_state| {
        let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
        if !root_hwnd.is_invalid() {
            let wnd_state = (); // TODO::

            // if proc.listening_cursor() {
            //     emit_cursor_event_from_message(backend.id, &proc, msg);
            // }

            // if proc.listening_keyboard() {
            //     emit_keyboard_event_from_message(backend.id, msg);
            // }
        };

        for f in msg_loop_state.proc_queue.lock().drain(..) {
            f(msg_loop_state);
        }
    });

    if !should_filter(msg) {
        return false;
    }

    unsafe {
        // Call TranslateMessage for char messages
        _ = TranslateMessage(msg);

        // Call Default WndProc so non client area works.
        if UNICODE {
            CallWindowProcW(
                Some(DefWindowProcA),
                msg.hwnd,
                msg.message,
                msg.wParam,
                msg.lParam,
            );
        } else {
            CallWindowProcA(
                Some(DefWindowProcW),
                msg.hwnd,
                msg.message,
                msg.wParam,
                msg.lParam,
            );
        }
    }

    true
}

#[inline]
fn emit_cursor_event_from_message(id: u32, proc: &WindowProcState, msg: &MSG) {
    match msg.message {
        msg::WM_MOUSEMOVE => {
            emit_cursor_move_event(id, proc, msg.lParam);
        }
        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            emit_cursor_event(id, proc, CursorAction::Left, true, msg.lParam);
        }
        msg::WM_LBUTTONUP => {
            emit_cursor_event(id, proc, CursorAction::Left, false, msg.lParam);
        }
        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            emit_cursor_event(id, proc, CursorAction::Right, true, msg.lParam);
        }
        msg::WM_RBUTTONUP => {
            emit_cursor_event(id, proc, CursorAction::Right, false, msg.lParam);
        }
        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            emit_cursor_event(id, proc, CursorAction::Middle, true, msg.lParam);
        }
        msg::WM_MBUTTONUP => {
            emit_cursor_event(id, proc, CursorAction::Middle, false, msg.lParam);
        }
        msg::WM_MOUSEWHEEL => {
            emit_cursor_scroll_event(id, proc, msg.wParam, msg.lParam, false);
        }
        msg::WM_MOUSEHWHEEL => {
            emit_cursor_scroll_event(id, proc, msg.wParam, msg.lParam, true);
        }
        _ => {}
    }
}

#[inline]
fn emit_keyboard_event_from_message(id: u32, msg: &MSG) {
    match msg.message {
        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            if let Some(key) = to_key(msg.lParam) {
                Backends::get().emit(keyboard_input(
                    id,
                    KeyboardInput::Key {
                        key,
                        state: KeyInputState::Pressed,
                    },
                ));
            }
        }
        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            if let Some(key) = to_key(msg.lParam) {
                Backends::get().emit(keyboard_input(
                    id,
                    KeyboardInput::Key {
                        key,
                        state: KeyInputState::Released,
                    },
                ));
            }
        }
        msg::WM_CHAR | msg::WM_SYSCHAR => {
            if let Some(ch) = char::from_u32(msg.wParam.0 as _) {
                Backends::get().emit(keyboard_input(id, KeyboardInput::Char(ch)));
            }
        }
        _ => {}
    }
}

#[inline]
fn parse_cursor_position(
    proc: &WindowProcState,
    lparam: LPARAM,
) -> (
    asdf_overlay_event::input::InputPosition,
    asdf_overlay_event::input::InputPosition,
) {
    use asdf_overlay_event::input::InputPosition;

    let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);
    let window = InputPosition {
        x: x as _,
        y: y as _,
    };
    let surface = InputPosition {
        x: window.x - proc.position.0,
        y: window.y - proc.position.1,
    };

    (surface, window)
}

#[inline]
fn emit_cursor_event(
    id: u32,
    proc: &WindowProcState,
    action: CursorAction,
    pressed: bool,
    lparam: LPARAM,
) {
    use asdf_overlay_event::input::{CursorEvent, CursorInputState};

    let (surface, window) = parse_cursor_position(proc, lparam);
    let state = if pressed {
        CursorInputState::Pressed {
            double_click: false,
        }
    } else {
        CursorInputState::Released
    };

    Backends::get().emit(OverlayEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Action { action, state },
            client: surface,
            window,
        })),
    });
}

#[inline]
fn emit_cursor_move_event(id: u32, proc: &WindowProcState, lparam: LPARAM) {
    use asdf_overlay_event::input::CursorEvent;

    let (surface, window) = parse_cursor_position(proc, lparam);

    Backends::get().emit(OverlayEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Move,
            client: surface,
            window,
        })),
    });
}

#[inline]
fn emit_cursor_scroll_event(
    id: u32,
    proc: &WindowProcState,
    wparam: WPARAM,
    lparam: LPARAM,
    horizontal: bool,
) {
    use asdf_overlay_event::input::CursorEvent;

    let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
    let (surface, window) = parse_cursor_position(proc, lparam);

    Backends::get().emit(OverlayEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Scroll {
                axis: if horizontal {
                    ScrollAxis::X
                } else {
                    ScrollAxis::Y
                },
                delta,
            },
            client: surface,
            window,
        })),
    });
}

#[inline]
fn is_cursor_message(message: u32) -> bool {
    matches!(
        message,
        msg::WM_MOUSEMOVE
            | msg::WM_LBUTTONDOWN
            | msg::WM_LBUTTONUP
            | msg::WM_LBUTTONDBLCLK
            | msg::WM_RBUTTONDOWN
            | msg::WM_RBUTTONUP
            | msg::WM_RBUTTONDBLCLK
            | msg::WM_MBUTTONDOWN
            | msg::WM_MBUTTONUP
            | msg::WM_MBUTTONDBLCLK
            | msg::WM_XBUTTONDOWN
            | msg::WM_XBUTTONUP
            | msg::WM_XBUTTONDBLCLK
            | msg::WM_MOUSEWHEEL
            | msg::WM_MOUSEHWHEEL
    )
}

#[inline]
fn is_keyboard_message(message: u32) -> bool {
    matches!(
        message,
        msg::WM_KEYDOWN
            | msg::WM_KEYUP
            | msg::WM_CHAR
            | msg::WM_SYSKEYDOWN
            | msg::WM_SYSKEYUP
            | msg::WM_SYSCHAR
    )
}

/// Filter input messages when blocking is enabled
#[inline]
fn should_filter(msg: &MSG) -> bool {
    if !is_cursor_message(msg.message) && !is_keyboard_message(msg.message) {
        return false;
    }

    Backends::get().input_blocked()
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
