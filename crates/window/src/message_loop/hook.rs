use core::{mem, sync::atomic::Ordering};
use std::time::Instant;

use asdf_overlay_hook::DetourHook;
use asdf_overlay_window_event::{
    Event, WindowEvent,
    input::{
        CursorAction, CursorEvent, CursorInput, CursorInputState, InputEvent, InputPosition, Key,
        KeyInputState, KeyboardInput, ScrollAxis,
    },
};
use once_cell::sync::OnceCell;
use tracing::{Level, debug, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::Threading::GetCurrentThreadId,
        UI::{
            Controls::{self, HOVER_DEFAULT},
            Input::KeyboardAndMouse::{
                MAPVK_VSC_TO_VK, MapVirtualKeyA, ReleaseCapture, SetCapture, TME_LEAVE,
                TRACKMOUSEEVENT, TrackMouseEvent,
            },
            WindowsAndMessaging::{
                self as msg, CallWindowProcA, CallWindowProcW, GA_ROOT, GetAncestor, MSG,
                PEEK_MESSAGE_REMOVE_TYPE, PM_REMOVE, TranslateMessage,
            },
        },
    },
    core::BOOL,
};

use crate::{Backends, window::ListenInputFlags};

windows::core::link!("user32.dll" "system" fn GetMessageA(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL);
windows::core::link!("user32.dll" "system" fn GetMessageW(lpmsg: *mut MSG, hwnd: HWND, wmsgfiltermin: u32, wmsgfiltermax: u32) -> BOOL);

windows::core::link!("user32.dll" "system" fn GetMessagePos() -> u32);

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

    pub(crate) get_message_pos: DetourHook<GetMessagePosFn>,

    pub(crate) peek_message_a: DetourHook<PeekMessageFn>,
    pub(crate) peek_message_w: DetourHook<PeekMessageFn>,
}

pub(crate) static HOOK: OnceCell<Hook> = OnceCell::new();

type GetMessageFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL;
type PeekMessageFn =
    unsafe extern "system" fn(*mut MSG, HWND, u32, u32, PEEK_MESSAGE_REMOVE_TYPE) -> BOOL;

type GetMessagePosFn = unsafe extern "system" fn() -> u32;

pub(crate) fn install() -> anyhow::Result<()> {
    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking GetMessageA");
        let get_message_a = DetourHook::attach(GetMessageA as _, hooked_get_message_a as _)?;

        debug!("hooking GetMessageW");
        let get_message_w = DetourHook::attach(GetMessageW as _, hooked_get_message_w as _)?;

        debug!("hooking GetMessagePos");
        let get_message_pos = DetourHook::attach(GetMessagePos as _, hooked_get_message_pos as _)?;

        debug!("hooking PeekMessageA");
        let peek_message_a = DetourHook::attach(PeekMessageA as _, hooked_peek_message_a as _)?;

        debug!("hooking PeekMessageW");
        let peek_message_w = DetourHook::attach(PeekMessageW as _, hooked_peek_message_w as _)?;

        Ok::<_, anyhow::Error>(Hook {
            get_message_a,
            get_message_w,
            get_message_pos,
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
    read_message::<UNICODE>(msg);

    if should_filter(msg) {
        filtered_proc::<UNICODE>(msg);
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
            HOOK.wait().peek_message_w.original_fn()(
                msg,
                hwnd,
                wmsgfiltermin,
                wmsgfiltermax,
                remove,
            )
        } else {
            HOOK.wait().peek_message_a.original_fn()(
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
    if remove.contains(PM_REMOVE) {
        read_message::<UNICODE>(msg);
    }

    if should_filter(msg) {
        filtered_proc::<UNICODE>(msg);
        msg.message = msg::WM_NULL;
    }
    original_read
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_message_a(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageA called");
    get_message::<false>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageW called");
    get_message::<true>(lpmsg, hwnd, wmsgfiltermin, wmsgfiltermax)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_get_message_pos() -> u32 {
    trace!("GetMessagePos called");
    if !Backends::get().input_blocked() {
        return unsafe { HOOK.wait().get_message_pos.original_fn()() };
    }

    0
}

#[tracing::instrument(level = Level::TRACE)]
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

#[tracing::instrument(level = Level::TRACE)]
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
fn read_message<const UNICODE: bool>(msg: &MSG) {
    let id = unsafe { GetCurrentThreadId() };

    let backends = Backends::get();

    if msg.message == msg::WM_QUIT {
        backends.cleanup_message_loop(id);
    }

    backends.message_loop_state(id, move |msg_loop_state| {
        let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
        if !root_hwnd.is_invalid() {
            let window_id = root_hwnd.0 as _;

            let input_blocked = backends.input_blocked();
            let input_flags = backends.window_state(window_id, |state| state.input_flags());

            if input_blocked || input_flags.contains(ListenInputFlags::CURSOR) {
                emit_cursor_event_from_message(window_id, msg);
            }

            if input_blocked || input_flags.contains(ListenInputFlags::KEYBOARD) {
                emit_keyboard_event_from_message(window_id, msg);
            }
        };

        for f in msg_loop_state.proc_queue.lock().drain(..) {
            f(msg_loop_state);
        }
    });
}

/// Process when the message is filtered.
fn filtered_proc<const UNICODE: bool>(msg: &MSG) {
    unsafe {
        // Call TranslateMessage for char messages
        _ = TranslateMessage(msg);

        if msg.hwnd.is_invalid() {
            return;
        }

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
}

#[inline]
fn emit_cursor_event_from_message(id: u32, msg: &MSG) {
    match msg.message {
        msg::WM_MOUSEMOVE => {
            cursor_move(id, msg.lParam);
        }

        Controls::WM_MOUSELEAVE => {
            cursor_leave(id);
        }

        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            cursor_action(id, CursorAction::Left, true, msg.lParam);
        }
        msg::WM_LBUTTONUP => {
            cursor_action(id, CursorAction::Left, false, msg.lParam);
        }
        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            cursor_action(id, CursorAction::Right, true, msg.lParam);
        }
        msg::WM_RBUTTONUP => {
            cursor_action(id, CursorAction::Right, false, msg.lParam);
        }
        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            cursor_action(id, CursorAction::Middle, true, msg.lParam);
        }
        msg::WM_MBUTTONUP => {
            cursor_action(id, CursorAction::Middle, false, msg.lParam);
        }

        msg::WM_MOUSEWHEEL => {
            cursor_scroll(id, msg.wParam, msg.lParam, false);
        }
        msg::WM_MOUSEHWHEEL => {
            cursor_scroll(id, msg.wParam, msg.lParam, true);
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

fn parse_cursor_position(lparam: LPARAM) -> InputPosition {
    let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);
    InputPosition {
        x: x as _,
        y: y as _,
    }
}

#[inline]
fn cursor_action(hwnd: u32, action: CursorAction, pressed: bool, lparam: LPARAM) {
    let pos = parse_cursor_position(lparam);

    if pressed {
        unsafe { SetCapture(HWND(hwnd as _)) };
    } else {
        _ = unsafe { ReleaseCapture() };
    }

    let index = match action {
        CursorAction::Left => 0,
        CursorAction::Right => 1,
        CursorAction::Middle => 2,
        CursorAction::Back => 3,
        CursorAction::Forward => 4,
    };

    let state = if pressed {
        let double_click = Backends::get().window_state(hwnd, |state| {
            state.check_double_click(index, Instant::now())
        });

        CursorInputState::Pressed { double_click }
    } else {
        CursorInputState::Released
    };

    Backends::get().emit(Event::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Action { action, state },
            pos,
        })),
    });
}

#[inline]
fn cursor_move(hwnd: u32, lparam: LPARAM) {
    let pos = parse_cursor_position(lparam);

    let backends = Backends::get();
    backends.window_state(hwnd, |state| {
        if state.cursor_hovering.load(Ordering::Relaxed) {
            return;
        }
        state.cursor_hovering.store(true, Ordering::Relaxed);

        // track for leave event
        unsafe {
            _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: HWND(hwnd as _),
                dwHoverTime: HOVER_DEFAULT,
            });
        };

        backends.emit(Event::Window {
            id: hwnd,
            event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                event: CursorEvent::Enter,
                pos,
            })),
        });
    });

    backends.emit(Event::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Move,
            pos,
        })),
    });
}

fn cursor_leave(id: u32) {
    let backends = Backends::get();
    let pos = {
        let screen_pos = unsafe { HOOK.wait().get_message_pos.original_fn()() };
        let [x, y] = bytemuck::cast::<_, [i16; 2]>(screen_pos);
        let mut point = POINT {
            x: x as _,
            y: y as _,
        };

        unsafe {
            _ = ScreenToClient(HWND(id as _), &mut point);
        }

        InputPosition {
            x: point.x,
            y: point.y,
        }
    };

    backends.window_state(id, |state| {
        if !state.cursor_hovering.load(Ordering::Relaxed) {
            return;
        }
        state.cursor_hovering.store(false, Ordering::Relaxed);

        backends.emit(Event::Window {
            id,
            event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                event: CursorEvent::Leave,
                pos,
            })),
        });
    });
}

#[inline]
fn cursor_scroll(hwnd: u32, wparam: WPARAM, lparam: LPARAM, horizontal: bool) {
    let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
    let pos = parse_cursor_position(lparam);

    Backends::get().emit(Event::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
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
fn is_filter_target(message: u32) -> bool {
    matches!(
        message,
        // Block WM_POINTER* by forwarding to DefWindowProc, which converts them
        // to legacy WM_LBUTTON*/WM_MOUSEMOVE/WM_MOUSEWHEEL messages.
        // Those legacy messages then re-enter this WndProc where they are
        // emitted to the message loop and blocked.
        msg::WM_POINTERUPDATE
            | msg::WM_POINTERDOWN
            | msg::WM_POINTERUP
            | msg::WM_POINTERENTER
            | msg::WM_POINTERLEAVE
            | msg::WM_POINTERACTIVATE
            | msg::WM_POINTERCAPTURECHANGED
            | msg::WM_POINTERWHEEL
            | msg::WM_POINTERHWHEEL

            // Mouse messages
            | msg::WM_MOUSEMOVE
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
            | Controls::WM_MOUSELEAVE
            | msg::WM_XBUTTONUP
            | msg::WM_XBUTTONDBLCLK
            | msg::WM_MOUSEWHEEL
            | msg::WM_MOUSEHWHEEL

            // Keyboard messages
            | msg::WM_KEYDOWN
            | msg::WM_KEYUP
            | msg::WM_CHAR
            | msg::WM_SYSKEYDOWN
            | msg::WM_SYSKEYUP
            | msg::WM_SYSCHAR

            // Raw input messages
            | msg::WM_INPUT
    )
}

/// Filter input messages when blocking is enabled
#[inline]
fn should_filter(msg: &MSG) -> bool {
    if !is_filter_target(msg.message) {
        return false;
    }

    Backends::get().input_blocked()
}

#[inline(always)]
fn keyboard_input(id: u32, input: KeyboardInput) -> Event {
    Event::Window {
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
