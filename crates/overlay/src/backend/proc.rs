use super::WindowBackend;
use crate::{
    app::Overlay,
    backend::{BACKENDS, Backends, CursorState},
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
use utf16string::{LittleEndian, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::{
            Ime::{GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext},
            KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        },
        WindowsAndMessaging::{
            self as msg, CallNextHookEx, CallWindowProcA, DefWindowProcA, GA_ROOT, GetAncestor,
            HHOOK, IDC_ARROW, LoadCursorW, MSG, SetCursor, UnhookWindowsHookEx, WM_NCDESTROY,
            WM_NULL, WM_QUIT, XBUTTON1,
        },
    },
};

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

    match msg {
        // reset key states on deactivate
        msg::WM_ACTIVATEAPP => {
            if wparam.0 == 0 {
                backend.reset_key_states();
            }
        }

        msg::WM_WINDOWPOSCHANGED => {
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

        _ => {}
    }

    if backend.capturing_input() {
        match msg {
            // set arrow cursor in client area
            msg::WM_SETCURSOR
                if {
                    let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
                    area == 1
                } =>
            unsafe {
                SetCursor(LoadCursorW(None, IDC_ARROW).ok());
                return LRESULT(1);
            },

            // stop input capture when user request to
            msg::WM_CLOSE => {
                backend.set_input_capture(false);
                return LRESULT(0);
            }

            // ignore client cursor inputs
            msg::WM_LBUTTONDBLCLK
            | msg::WM_LBUTTONDOWN
            | msg::WM_LBUTTONUP
            | msg::WM_MBUTTONDBLCLK
            | msg::WM_MBUTTONDOWN
            | msg::WM_MBUTTONUP
            | msg::WM_MOUSEMOVE
            | msg::WM_RBUTTONDBLCLK
            | msg::WM_RBUTTONDOWN
            | msg::WM_RBUTTONUP => return LRESULT(1),

            // ignore x button client cursor inputs
            msg::WM_XBUTTONDBLCLK | msg::WM_XBUTTONDOWN | msg::WM_XBUTTONUP => return LRESULT(1),

            // handle system key input
            msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN | msg::WM_KEYUP | msg::WM_SYSKEYUP => {
                return unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) };
            }

            // ignore raw input (ignoring in hook leak handle)
            msg::WM_INPUT => {}

            _ => {}
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

    if backend.capturing_input() {
        process_input_capture(backend, msg);
    }
}

#[inline]
fn process_input_capture(backend: &mut WindowBackend, msg: &mut MSG) {
    // emit cursor input and redirect to window hwnd
    macro_rules! emit_cursor_input {
        ($action:expr, $state:expr $(,)?) => {{
            Overlay::emit_event(cursor_input(
                backend.hwnd,
                msg.lParam,
                CursorEvent::Action {
                    state: $state,
                    action: $action,
                },
            ));

            redirect_msg_to(HWND(backend.hwnd as _), msg);
            return;
        }};
    }

    macro_rules! emit_key_input {
        ($state:expr $(,)?) => {{
            if let Some(key) = to_key(msg.wParam, msg.lParam) {
                Overlay::emit_event(keyboard_input(
                    backend.hwnd,
                    KeyboardInput::Key { key, state: $state },
                ));
            }
        }};
    }

    match msg.message {
        msg::WM_LBUTTONDOWN => emit_cursor_input!(CursorAction::Left, InputState::Pressed),
        msg::WM_MBUTTONDOWN => emit_cursor_input!(CursorAction::Middle, InputState::Pressed),
        msg::WM_RBUTTONDOWN => emit_cursor_input!(CursorAction::Right, InputState::Pressed),
        msg::WM_XBUTTONDOWN => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(msg.lParam.0 as u32);
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
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(msg.lParam.0 as u32);
            emit_cursor_input!(
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed
            );
        }

        Controls::WM_MOUSELEAVE => {
            backend.cursor_state = CursorState::Outside;
            Overlay::emit_event(cursor_input(backend.hwnd, msg.lParam, CursorEvent::Leave));
        }

        msg::WM_MOUSEMOVE => {
            let [x, y] = bytemuck::cast::<_, [i16; 2]>(msg.lParam.0 as u32);

            match backend.cursor_state {
                CursorState::Inside(ref mut old_x, ref mut old_y) => {
                    *old_x = x;
                    *old_y = y;
                }
                CursorState::Outside => {
                    backend.cursor_state = CursorState::Inside(x, y);
                    Overlay::emit_event(ClientEvent::Window {
                        hwnd: backend.hwnd,
                        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                            event: CursorEvent::Enter,
                            x,
                            y,
                        })),
                    });

                    // track for leave event
                    _ = unsafe {
                        TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: HWND(backend.hwnd as _),
                            dwHoverTime: HOVER_DEFAULT,
                        })
                    };
                }
            }

            Overlay::emit_event(ClientEvent::Window {
                hwnd: backend.hwnd,
                event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                    event: CursorEvent::Move,
                    x,
                    y,
                })),
            });
        }

        msg::WM_MOUSEWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(msg.wParam.0 as u32);
            Overlay::emit_event(cursor_input(
                backend.hwnd,
                msg.lParam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));
        }

        msg::WM_MOUSEHWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(msg.wParam.0 as u32);
            Overlay::emit_event(cursor_input(
                backend.hwnd,
                msg.lParam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                },
            ));
        }

        // ignore remaining cursor inputs
        msg::WM_LBUTTONDBLCLK
        | msg::WM_MBUTTONDBLCLK
        | msg::WM_RBUTTONDBLCLK
        | msg::WM_XBUTTONDBLCLK => {}

        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            emit_key_input!(InputState::Pressed);
            redirect_msg_to(HWND(backend.hwnd as _), msg);
            return;
        }
        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            emit_key_input!(InputState::Released);
            redirect_msg_to(HWND(backend.hwnd as _), msg);
            return;
        }

        msg::WM_CHAR => {
            if let Some(ch) = char::from_u32(msg.wParam.0 as _) {
                Overlay::emit_event(keyboard_input(backend.hwnd, KeyboardInput::Char(ch)));
            }
        }
        msg::WM_IME_COMPOSITION if msg.lParam.0 as u32 == GCS_RESULTSTR.0 => {
            if let Some(str) = get_ime_string(HWND(backend.hwnd as _)) {
                for ch in str.chars() {
                    Overlay::emit_event(keyboard_input(backend.hwnd, KeyboardInput::Char(ch)));
                }
            }
        }

        // ignore remaining keyboard inputs
        msg::WM_APPCOMMAND
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR => {}

        _ => return,
    }

    // nullify handled message by default
    *msg = MSG {
        hwnd: HWND::default(),
        message: WM_NULL,
        wParam: WPARAM(0),
        lParam: LPARAM(0),
        time: msg.time,
        pt: msg.pt,
    };
}

#[tracing::instrument]
pub(super) unsafe extern "system" fn call_wnd_proc_hook(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("GetMsgProc hook called");

    // only check message being removed from the queue
    if wparam.0 == 1 && ncode == 0 {
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

#[inline]
fn redirect_msg_to(hwnd: HWND, msg: &mut MSG) {
    if msg.hwnd != hwnd {
        msg.hwnd = hwnd;
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
