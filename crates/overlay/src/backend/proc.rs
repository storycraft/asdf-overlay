use super::WindowBackend;
use crate::{
    app::Overlay,
    backend::{BACKENDS, Backends},
    util::get_client_size,
};
use asdf_overlay_common::{
    event::{
        ClientEvent, WindowEvent,
        input::{
            CursorAction, CursorEvent, CursorInput, InputEvent, InputState, KeyboardInput,
            ScrollAxis,
        },
    },
    key::Key,
};
use core::mem;
use scopeguard::defer;
use tracing::trace;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::KeyboardAndMouse::{TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        WindowsAndMessaging::{
            self as msg, CallNextHookEx, CallWindowProcA, DefWindowProcA, GA_ROOT, GetAncestor,
            HC_ACTION, HHOOK, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, SetCursor,
            UnhookWindowsHookEx, WM_CLOSE, WM_NCDESTROY, WM_NULL, WM_QUIT, WM_WINDOWPOSCHANGED,
            XBUTTON1,
        },
    },
};

#[inline]
fn filter_input(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        // handle hit test
        msg::WM_NCHITTEST => {
            return Some(unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) });
        }

        // show arrow cursor in client area
        msg::WM_SETCURSOR => {
            let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);

            if area == 1 {
                unsafe {
                    SetCursor(LoadCursorW(None, IDC_ARROW).ok());
                }

                return Some(LRESULT(1));
            } else {
                return None;
            }
        }

        // ignore cursor inputs
        msg::WM_LBUTTONDBLCLK
        | msg::WM_LBUTTONDOWN
        | msg::WM_LBUTTONUP
        | msg::WM_MBUTTONDBLCLK
        | msg::WM_MBUTTONDOWN
        | msg::WM_MBUTTONUP
        | msg::WM_MOUSEACTIVATE
        | Controls::WM_MOUSEHOVER
        | msg::WM_MOUSEHWHEEL
        | Controls::WM_MOUSELEAVE
        | msg::WM_MOUSEMOVE
        | msg::WM_MOUSEWHEEL
        | msg::WM_RBUTTONDBLCLK
        | msg::WM_RBUTTONDOWN
        | msg::WM_RBUTTONUP => {}

        // ignore cursor inputs but special xbutton inputs should return 1 if handled
        msg::WM_XBUTTONDBLCLK | msg::WM_XBUTTONDOWN | msg::WM_XBUTTONUP => return Some(LRESULT(1)),

        // ignore keyboard inputs
        msg::WM_APPCOMMAND
        | msg::WM_CHAR
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_KEYDOWN
        | msg::WM_KEYUP
        | msg::WM_KILLFOCUS
        | msg::WM_SETFOCUS
        | msg::WM_SYSDEADCHAR
        | msg::WM_SYSKEYDOWN
        | msg::WM_SYSKEYUP
        | msg::WM_UNICHAR => {}

        // ignore raw input
        msg::WM_INPUT => {}

        // ignore ime messages
        msg::WM_IME_CHAR
        | msg::WM_IME_COMPOSITION
        | msg::WM_IME_COMPOSITIONFULL
        | msg::WM_IME_CONTROL
        | msg::WM_IME_ENDCOMPOSITION
        | msg::WM_IME_KEYDOWN
        | msg::WM_IME_KEYUP
        | msg::WM_IME_NOTIFY
        | msg::WM_IME_REQUEST
        | msg::WM_IME_SELECT
        | msg::WM_IME_SETCONTEXT
        | msg::WM_IME_STARTCOMPOSITION => {}

        _ => return None,
    }

    Some(LRESULT(0))
}

#[tracing::instrument]
pub(super) unsafe extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc called");

    defer!({
        // cleanup backend
        if msg == WM_NCDESTROY {
            trace!("cleanup hwnd: {:?}", hwnd);
            Backends::remove_backend(hwnd);
        }
    });

    let mut backend = BACKENDS.map.get_mut(&(hwnd.0 as u32)).unwrap();
    if msg == WM_WINDOWPOSCHANGED {
        let new_size = get_client_size(hwnd).unwrap();
        if backend.size != new_size {
            backend.size = new_size;
            Overlay::emit_event(ClientEvent::Window {
                hwnd: backend.hwnd,
                event: WindowEvent::Resized {
                    width: backend.size.0,
                    height: backend.size.1,
                },
            });
        }
    }

    if backend.capturing_input() {
        if msg == WM_CLOSE {
            // stop input capture when user request to
            backend.set_input_capture(false);
            return LRESULT(0);
        } else if let Some(res) = filter_input(hwnd, msg, wparam, lparam) {
            return res;
        }
    }

    let original_proc = backend.original_proc;
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn process_call_wnd_proc_hook(backend: &mut WindowBackend, msg: &mut MSG) {
    match msg.message {
        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            if let Some(key) = to_key(msg.wParam, msg.lParam) {
                backend.update_key_state(key, true);
            }
        }

        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            if let Some(key) = to_key(msg.wParam, msg.lParam) {
                backend.update_key_state(key, false);
            }
        }

        _ => {}
    }

    if backend.capturing_input()
        && process_input_capture(backend.hwnd, msg.message, msg.wParam, msg.lParam)
    {
        *msg = MSG {
            hwnd: msg.hwnd,
            message: WM_NULL,
            wParam: WPARAM(0),
            lParam: LPARAM(0),
            time: msg.time,
            pt: msg.pt,
        };
    }
}

#[inline]
fn process_input_capture(hwnd: u32, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    #[inline(always)]
    fn cursor_input(hwnd: u32, lparam: LPARAM, event: CursorEvent) -> ClientEvent {
        let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

        ClientEvent::Window {
            hwnd,
            event: WindowEvent::Input(InputEvent::Cursor(CursorInput { event, x, y })),
        }
    }

    #[inline(always)]
    fn keyboard_input(hwnd: u32, input: KeyboardInput) -> ClientEvent {
        ClientEvent::Window {
            hwnd,
            event: WindowEvent::Input(InputEvent::Keyboard(input)),
        }
    }

    macro_rules! emit_cursor_input {
        ($action:expr, $state:expr $(,)?) => {{
            Overlay::emit_event(cursor_input(
                hwnd,
                lparam,
                CursorEvent::Action {
                    state: $state,
                    action: $action,
                },
            ));
        }};
    }

    macro_rules! emit_keyboard_input {
        ($state:expr $(,)?) => {{
            if let Some(key) = to_key(wparam, lparam) {
                Overlay::emit_event(keyboard_input(hwnd, KeyboardInput { key, state: $state }));
            }
        }};
    }

    match msg {
        msg::WM_LBUTTONDOWN => emit_cursor_input!(CursorAction::Left, InputState::Pressed),
        msg::WM_MBUTTONDOWN => emit_cursor_input!(CursorAction::Middle, InputState::Pressed),
        msg::WM_RBUTTONDOWN => emit_cursor_input!(CursorAction::Right, InputState::Pressed),
        msg::WM_XBUTTONDOWN => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            emit_cursor_input!(
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed
            );
        }

        msg::WM_LBUTTONUP => emit_cursor_input!(CursorAction::Left, InputState::Released),
        msg::WM_MBUTTONUP => emit_cursor_input!(CursorAction::Middle, InputState::Released),
        msg::WM_RBUTTONUP => emit_cursor_input!(CursorAction::Right, InputState::Released),
        msg::WM_XBUTTONUP => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            emit_cursor_input!(
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed
            );
        }

        Controls::WM_MOUSEHOVER => {
            Overlay::emit_event(cursor_input(hwnd, lparam, CursorEvent::Enter));
            return true;
        }
        Controls::WM_MOUSELEAVE => {
            Overlay::emit_event(cursor_input(hwnd, lparam, CursorEvent::Leave));
            return true;
        }

        msg::WM_MOUSEMOVE => {
            // track for leave and hover event
            _ = unsafe {
                TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_HOVER | TME_LEAVE,
                    hwndTrack: HWND(hwnd as _),
                    dwHoverTime: HOVER_DEFAULT,
                })
            };

            Overlay::emit_event(cursor_input(hwnd, lparam, CursorEvent::Move));
            return true;
        }

        msg::WM_MOUSEWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            Overlay::emit_event(cursor_input(
                hwnd,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));

            return true;
        }

        msg::WM_MOUSEHWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            Overlay::emit_event(cursor_input(
                hwnd,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                },
            ));

            return true;
        }

        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            emit_keyboard_input!(InputState::Pressed);
            return true;
        }
        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            emit_keyboard_input!(InputState::Released);
            return true;
        }

        _ => {}
    }

    false
}

#[tracing::instrument]
pub(super) unsafe extern "system" fn call_wnd_proc_hook(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("GetMsgProc hook called");

    if ncode == HC_ACTION as i32 && wparam.0 as u32 == PM_REMOVE.0 {
        let msg = unsafe { &mut *(lparam.0 as *mut MSG) };

        if msg.message == WM_QUIT {
            // remove hook
            if let Some((_, hhook)) = BACKENDS
                .thread_hook_map
                .remove(&unsafe { GetCurrentThreadId() })
            {
                _ = unsafe { UnhookWindowsHookEx(HHOOK(hhook as _)) };
            }
        }

        if !msg.hwnd.is_invalid() {
            let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
            _ = Backends::with_backend(root_hwnd, |backend| {
                process_call_wnd_proc_hook(backend, msg)
            });
        }
    }

    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

#[inline]
fn to_key(wparam: WPARAM, lparam: LPARAM) -> Option<Key> {
    let [_, _, _, flags] = bytemuck::cast::<_, [u8; 4]>(lparam.0 as u32);
    Key::new(wparam.0 as _, flags & 0x01 == 0x01)
}
