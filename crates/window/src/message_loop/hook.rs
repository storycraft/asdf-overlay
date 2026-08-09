use core::time::Duration;
use std::time::Instant;

use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{
                GetDoubleClickTime, MAPVK_VSC_TO_VK, MapVirtualKeyA, ReleaseCapture, SetCapture,
            },
            WindowsAndMessaging::{
                self as msg, CallWindowProcA, CallWindowProcW, GA_ROOT, GetAncestor, MSG,
                PEEK_MESSAGE_REMOVE_TYPE, PM_REMOVE, TranslateMessage,
            },
        },
    },
    core::BOOL,
};

use crate::{
    Backends,
    event::{
        BackendEvent, WindowEvent,
        input::{
            CursorAction, CursorEvent, CursorInput, CursorInputState, InputEvent, InputPosition,
            Key, KeyInputState, KeyboardInput, ScrollAxis,
        },
    },
    window::ListenInputFlags,
};

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
    get_message::<false>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax)
}

#[tracing::instrument]
extern "system" fn hooked_get_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageW called");
    get_message::<true>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax)
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
            let window_id = root_hwnd.0 as _;

            let input_flags =
                Backends::get().window_state(window_id, |wnd_state| wnd_state.input_flags);

            if input_flags.contains(ListenInputFlags::CURSOR) {
                emit_cursor_event_from_message(window_id, msg);
            }

            if input_flags.contains(ListenInputFlags::KEYBOARD) {
                emit_keyboard_event_from_message(window_id, msg);
            }
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
fn emit_cursor_event_from_message(id: u32, msg: &MSG) {
    match msg.message {
        msg::WM_POINTERUPDATE => {
            emit_cursor_move_event(id, msg.lParam, msg.wParam);
        }
        msg::WM_POINTERDOWN => {
            emit_cursor_action_event(id, true, msg.wParam, msg.lParam);
        }
        msg::WM_POINTERUP => {
            emit_cursor_action_event(id, false, msg.wParam, msg.lParam);
        }
        msg::WM_POINTERWHEEL => {
            emit_cursor_scroll_event(id, msg.wParam, msg.lParam, false);
        }
        msg::WM_POINTERHWHEEL => {
            emit_cursor_scroll_event(id, msg.wParam, msg.lParam, true);
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

fn parse_cursor(wparam: WPARAM) -> (u16, bool, PointerKeys) {
    const POINTER_FLAG_PRIMARY: u16 = 0x2000;

    let [id, keys] = bytemuck::cast::<_, [u16; 2]>(wparam.0 as u32);
    (
        id,
        keys & POINTER_FLAG_PRIMARY != 0,
        PointerKeys::from_bits_truncate(keys),
    )
}

fn parse_cursor_position(lparam: LPARAM) -> InputPosition {
    let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);
    InputPosition {
        x: x as _,
        y: y as _,
    }
}

#[inline]
fn emit_cursor_action_event(hwnd: u32, pressed: bool, wparam: WPARAM, lparam: LPARAM) {
    let (id, primary, keys) = parse_cursor(wparam);
    let pos = parse_cursor_position(lparam);

    if pressed {
        unsafe { SetCapture(HWND(hwnd as _)) };
    } else {
        _ = unsafe { ReleaseCapture() };
    }

    for key in keys.iter() {
        let (index, action) = match key {
            PointerKeys::FIRST_BUTTON => (0, CursorAction::Left),
            PointerKeys::SECOND_BUTTON => (1, CursorAction::Right),
            PointerKeys::THIRD_BUTTON => (2, CursorAction::Middle),
            PointerKeys::FOURTH_BUTTON => (3, CursorAction::Back),
            PointerKeys::FIFTH_BUTTON => (4, CursorAction::Forward),
            _ => continue,
        };

        let state = if pressed {
            let click_delta = Backends::get()
                .window_state(hwnd, |state| state.update_click_time(index, Instant::now()));
            let double_click =
                click_delta < Duration::from_millis(unsafe { GetDoubleClickTime() } as _);

            CursorInputState::Pressed { double_click }
        } else {
            CursorInputState::Released
        };

        Backends::get().emit(BackendEvent::Window {
            id: hwnd,
            event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                id,
                primary,
                event: CursorEvent::Action { action, state },
                pos,
            })),
        });
    }
}

#[inline]
fn emit_cursor_move_event(hwnd: u32, lparam: LPARAM, wparam: WPARAM) {
    let (id, primary, _) = parse_cursor(wparam);
    let pos = parse_cursor_position(lparam);

    Backends::get().emit(BackendEvent::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            id,
            primary,
            event: CursorEvent::Move,
            pos,
        })),
    });
}

#[inline]
fn emit_cursor_scroll_event(hwnd: u32, wparam: WPARAM, lparam: LPARAM, horizontal: bool) {
    let [id, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
    let pos = parse_cursor_position(lparam);

    Backends::get().emit(BackendEvent::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            id: id as _,
            primary: true,
            event: CursorEvent::Scroll {
                axis: if horizontal {
                    ScrollAxis::X
                } else {
                    ScrollAxis::Y
                },
                delta,
            },
            pos,
        })),
    });
}

#[inline]
fn is_cursor_message(message: u32) -> bool {
    matches!(
        message,
        msg::WM_POINTERUPDATE
            | msg::WM_POINTERDOWN
            | msg::WM_POINTERUP
            | msg::WM_POINTERENTER
            | msg::WM_POINTERLEAVE
            | msg::WM_POINTERACTIVATE
            | msg::WM_POINTERCAPTURECHANGED
            | msg::WM_POINTERWHEEL
            | msg::WM_POINTERHWHEEL
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
fn keyboard_input(id: u32, input: KeyboardInput) -> BackendEvent {
    BackendEvent::Window {
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

bitflags::bitflags! {
    #[derive(PartialEq)]
    struct PointerKeys: u16 {
        const FIRST_BUTTON = 0x0010;
        const SECOND_BUTTON = 0x0020;
        const THIRD_BUTTON = 0x0040;
        const FOURTH_BUTTON = 0x0080;
        const FIFTH_BUTTON = 0x0100;
    }
}
