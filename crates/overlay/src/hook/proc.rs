mod input;

use asdf_overlay_event::{
    OverlayEvent, WindowEvent,
    input::{CursorAction, CursorInput, InputEvent, Key, KeyInputState, KeyboardInput, ScrollAxis},
};
use asdf_overlay_hook::DetourHook;
use core::cell::Cell;
use once_cell::sync::OnceCell;
use scopeguard::defer;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK, MapVirtualKeyA},
            WindowsAndMessaging::{
                self as msg, CallWindowProcA, CallWindowProcW, GA_ROOT, GetAncestor, MSG,
                PEEK_MESSAGE_REMOVE_TYPE, PM_REMOVE,
            },
        },
    },
    core::BOOL,
};

use crate::{
    backend::{Backends, WindowBackend, window::WindowProcData},
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
}

struct Hook {
    get_message_a: DetourHook<GetMessageFn>,
    get_message_w: DetourHook<GetMessageFn>,

    peek_message_a: DetourHook<PeekMessageFn>,
    peek_message_w: DetourHook<PeekMessageFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

type GetMessageFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL;
type PeekMessageFn =
    unsafe extern "system" fn(*mut MSG, HWND, u32, u32, PEEK_MESSAGE_REMOVE_TYPE) -> BOOL;

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

        Ok::<_, anyhow::Error>(Hook {
            get_message_a,
            get_message_w,

            peek_message_a,
            peek_message_w,
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

unsafe fn process_read_message<const UNICODE: bool>(
    msg: *mut MSG,
    reader: impl Fn(*mut MSG) -> bool,
) -> bool {
    set_message_read(move || {
        loop {
            let ret = reader(msg);
            if !ret {
                return ret;
            }
            let msg = unsafe { &mut *msg };

            // For SDL games: Emit events BEFORE filtering so overlay gets them
            // even if we filter the message to block SDL
            emit_input_event_from_message(msg);
            if should_filter_message(msg) {
                if UNICODE {
                    unsafe {
                        CallWindowProcW(None, msg.hwnd, msg.message, msg.wParam, msg.lParam);
                    }
                } else {
                    unsafe {
                        CallWindowProcA(None, msg.hwnd, msg.message, msg.wParam, msg.lParam);
                    }
                }
                continue;
            }

            on_message_read(msg);
            return ret;
        }
    })
}

unsafe fn process_peek_message(
    msg: *mut MSG,
    remove: bool,
    reader: impl Fn(*mut MSG) -> bool,
) -> bool {
    set_message_read(move || {
        let ret = reader(msg);
        if !ret {
            return ret;
        }
        let msg = unsafe { &mut *msg };

        if remove {
            // For SDL games: Emit events BEFORE filtering so overlay gets them
            // even if we filter the message to block SDL
            emit_input_event_from_message(msg);
            on_message_read(msg);
        }

        if should_filter_message(msg) {
            msg.message = msg::WM_NULL;
        }
        ret
    })
}

#[tracing::instrument]
extern "system" fn hooked_get_message_a(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageA called");

    unsafe {
        process_read_message::<false>(lpmsg, |msg| {
            HOOK.wait().get_message_a.original_fn()(msg, hwnd, wmsgfiltermin, wmsgfiltermax)
                .as_bool()
        })
    }
    .into()
}

#[tracing::instrument]
extern "system" fn hooked_get_message_w(
    lpmsg: *mut MSG,
    hwnd: HWND,
    wmsgfiltermin: u32,
    wmsgfiltermax: u32,
) -> BOOL {
    trace!("GetMessageW called");

    unsafe {
        process_read_message::<true>(lpmsg, |msg| {
            HOOK.wait().get_message_w.original_fn()(msg, hwnd, wmsgfiltermin, wmsgfiltermax)
                .as_bool()
        })
    }
    .into()
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

    unsafe {
        process_peek_message(lpmsg, wremovemsg.contains(PM_REMOVE), |msg| {
            HOOK.wait().peek_message_a.original_fn()(
                msg,
                hwnd,
                wmsgfiltermin,
                wmsgfiltermax,
                wremovemsg,
            )
            .as_bool()
        })
    }
    .into()
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

    unsafe {
        process_peek_message(lpmsg, wremovemsg.contains(PM_REMOVE), |msg| {
            HOOK.wait().peek_message_w.original_fn()(
                msg,
                hwnd,
                wmsgfiltermin,
                wmsgfiltermax,
                wremovemsg,
            )
            .as_bool()
        })
    }
    .into()
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

/// Emit input events for SDL games that use PeekMessage instead of DispatchMessage
#[inline]
fn emit_input_event_from_message(msg: &MSG) {
    with_root_backend(msg, |backend| {
        let proc = backend.proc.lock();

        if proc.listening_cursor() {
            emit_cursor_event_from_message(backend.id, &proc, msg);
        }

        if proc.listening_keyboard() {
            emit_keyboard_event_from_message(backend.id, msg);
        }
    });
}

#[inline]
fn emit_cursor_event_from_message(id: u32, proc: &WindowProcData, msg: &MSG) {
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
                OverlayEventSink::emit(keyboard_input(
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
                OverlayEventSink::emit(keyboard_input(
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
                OverlayEventSink::emit(keyboard_input(id, KeyboardInput::Char(ch)));
            }
        }
        _ => {}
    }
}

#[inline]
fn parse_cursor_position(
    proc: &WindowProcData,
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
    proc: &WindowProcData,
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

    OverlayEventSink::emit(OverlayEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event: CursorEvent::Action { action, state },
            client: surface,
            window,
        })),
    });
}

#[inline]
fn emit_cursor_move_event(id: u32, proc: &WindowProcData, lparam: LPARAM) {
    use asdf_overlay_event::input::CursorEvent;

    let (surface, window) = parse_cursor_position(proc, lparam);

    OverlayEventSink::emit(OverlayEvent::Window {
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
    proc: &WindowProcData,
    wparam: WPARAM,
    lparam: LPARAM,
    horizontal: bool,
) {
    use asdf_overlay_event::input::CursorEvent;

    let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
    let (surface, window) = parse_cursor_position(proc, lparam);

    OverlayEventSink::emit(OverlayEvent::Window {
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

const CURSOR_MESSAGES: &[u32] = &[
    msg::WM_MOUSEMOVE,
    msg::WM_LBUTTONDOWN,
    msg::WM_LBUTTONUP,
    msg::WM_LBUTTONDBLCLK,
    msg::WM_RBUTTONDOWN,
    msg::WM_RBUTTONUP,
    msg::WM_RBUTTONDBLCLK,
    msg::WM_MBUTTONDOWN,
    msg::WM_MBUTTONUP,
    msg::WM_MBUTTONDBLCLK,
    msg::WM_XBUTTONDOWN,
    msg::WM_XBUTTONUP,
    msg::WM_XBUTTONDBLCLK,
    msg::WM_MOUSEWHEEL,
    msg::WM_MOUSEHWHEEL,
];

const KEYBOARD_MESSAGES: &[u32] = &[
    msg::WM_KEYDOWN,
    msg::WM_KEYUP,
    msg::WM_CHAR,
    msg::WM_SYSKEYDOWN,
    msg::WM_SYSKEYUP,
    msg::WM_SYSCHAR,
];

#[inline]
fn is_cursor_message(message: u32) -> bool {
    CURSOR_MESSAGES.contains(&message)
}

#[inline]
fn is_keyboard_message(message: u32) -> bool {
    KEYBOARD_MESSAGES.contains(&message)
}

/// Filter input messages when blocking is enabled
#[inline]
fn should_filter_message(msg: &MSG) -> bool {
    if !is_cursor_message(msg.message) && !is_keyboard_message(msg.message) {
        return false;
    }

    with_root_backend(msg, |backend| backend.proc.lock().input_blocking()).unwrap_or(false)
}

#[inline]
fn with_root_backend<R>(msg: &MSG, f: impl FnOnce(&WindowBackend) -> R) -> Option<R> {
    let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
    if root_hwnd.is_invalid() {
        return None;
    }

    Backends::with_backend(root_hwnd.0 as _, f)
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
