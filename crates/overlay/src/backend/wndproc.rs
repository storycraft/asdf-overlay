use super::WindowBackend;
use crate::{app::Overlay, backend::BACKENDS, util::get_client_size};
use asdf_overlay_common::event::{
    ClientEvent, WindowEvent,
    input::{CursorAction, CursorInput, InputEvent, InputState, KeyboardInput, ScrollAxis},
};
use core::{mem, num::NonZeroU8};
use scopeguard::defer;
use tracing::trace;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::KeyboardAndMouse::{
            self, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VIRTUAL_KEY,
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, WHEEL_DELTA, WM_CLOSE, WM_NCDESTROY,
            WM_WINDOWPOSCHANGED, XBUTTON1,
        },
    },
};

#[inline]
fn process_wnd_proc(
    backend: &mut WindowBackend,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match msg {
        WM_WINDOWPOSCHANGED => {
            let new_size = get_client_size(hwnd).unwrap();
            if backend.size != new_size {
                backend.size = new_size;
                Overlay::emit_event(ClientEvent::Window {
                    hwnd: hwnd.0 as u32,
                    event: WindowEvent::Resized {
                        width: backend.size.0,
                        height: backend.size.1,
                    },
                });
            }
        }

        WM_NCDESTROY => {
            Overlay::emit_event(ClientEvent::Window {
                hwnd: hwnd.0 as u32,
                event: WindowEvent::Destroyed,
            });
        }

        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => {
            if let Some(key) = NonZeroU8::new(get_distinguished_keycode(wparam, lparam)) {
                backend.update_key_state(key, true);
            }
        }

        msg::WM_KEYUP | msg::WM_SYSKEYUP => {
            if let Some(key) = NonZeroU8::new(get_distinguished_keycode(wparam, lparam)) {
                backend.update_key_state(key, false);
            }
        }

        _ => {}
    }

    if backend.capturing_input() {
        if msg == WM_CLOSE {
            // stop input capture when user request to
            backend.set_input_capture(false);
            return Some(LRESULT(0));
        } else if let Some(res) = process_input_capture(hwnd, msg, wparam, lparam) {
            return Some(res);
        }
    }

    None
}

#[inline]
fn process_input_capture(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    #[inline(always)]
    fn input(hwnd: HWND, input: InputEvent) -> ClientEvent {
        ClientEvent::Window {
            hwnd: hwnd.0 as u32,
            event: WindowEvent::Input(input),
        }
    }

    macro_rules! emit_cursor_input {
        ($action:expr, $state:expr $(,)?) => {{
            let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

            Overlay::emit_event(input(
                hwnd,
                InputEvent::Cursor(CursorInput::Action {
                    state: $state,
                    action: $action,
                    x,
                    y,
                }),
            ));
        }};
    }

    macro_rules! emit_keyboard_input {
        ($state:expr $(,)?) => {{
            Overlay::emit_event(input(
                hwnd,
                InputEvent::Keyboard(KeyboardInput {
                    key: get_distinguished_keycode(wparam, lparam),
                    state: $state,
                }),
            ));
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

            // for xbutton it should return 1 for handled
            return Some(LRESULT(1));
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

            // for xbutton it should return 1 for handled
            return Some(LRESULT(1));
        }

        Controls::WM_MOUSEHOVER => {
            Overlay::emit_event(input(hwnd, InputEvent::Cursor(CursorInput::Enter)));
        }
        Controls::WM_MOUSELEAVE => {
            Overlay::emit_event(input(hwnd, InputEvent::Cursor(CursorInput::Leave)));
        }

        msg::WM_MOUSEMOVE => {
            // track for leave and hover event
            _ = unsafe {
                TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_HOVER | TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: HOVER_DEFAULT,
                })
            };

            let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);
            Overlay::emit_event(input(hwnd, InputEvent::Cursor(CursorInput::Move { x, y })));
        }

        msg::WM_MOUSEWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            let delta = delta as f32 / WHEEL_DELTA as f32;

            Overlay::emit_event(input(
                hwnd,
                InputEvent::Cursor(CursorInput::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                }),
            ));
        }

        msg::WM_MOUSEHWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            let delta = delta as f32 / WHEEL_DELTA as f32;

            Overlay::emit_event(input(
                hwnd,
                InputEvent::Cursor(CursorInput::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                }),
            ));
        }

        // handle hit test
        msg::WM_NCHITTEST => {
            return Some(unsafe { DefWindowProcA(hwnd, msg, wparam, lparam) });
        }

        // ignore other cursor inputs
        msg::WM_LBUTTONDBLCLK
        | msg::WM_MBUTTONDBLCLK
        | msg::WM_MOUSEACTIVATE
        | msg::WM_RBUTTONDBLCLK
        | msg::WM_XBUTTONDBLCLK => {}

        msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN => emit_keyboard_input!(InputState::Pressed),
        msg::WM_KEYUP | msg::WM_SYSKEYUP => emit_keyboard_input!(InputState::Released),

        // ignore other keyboard inputs
        msg::WM_APPCOMMAND
        | msg::WM_CHAR
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_KILLFOCUS
        | msg::WM_SETFOCUS
        | msg::WM_SYSDEADCHAR
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

// get distinguished key code
fn get_distinguished_keycode(wparam: WPARAM, lparam: LPARAM) -> u8 {
    let code = wparam.0 as u16;
    let flags = lparam.0 as u32;

    match VIRTUAL_KEY(code) {
        KeyboardAndMouse::VK_SHIFT if flags & 0x01000000 != 0 => {
            KeyboardAndMouse::VK_RSHIFT.0 as u8
        }
        KeyboardAndMouse::VK_CONTROL if flags & 0x01000000 != 0 => {
            KeyboardAndMouse::VK_RCONTROL.0 as u8
        }
        KeyboardAndMouse::VK_MENU if flags & 0x01000000 != 0 => KeyboardAndMouse::VK_RMENU.0 as u8,

        KeyboardAndMouse::VK_SHIFT => KeyboardAndMouse::VK_LSHIFT.0 as u8,
        KeyboardAndMouse::VK_CONTROL => KeyboardAndMouse::VK_LCONTROL.0 as u8,
        KeyboardAndMouse::VK_MENU => KeyboardAndMouse::VK_LMENU.0 as u8,

        VIRTUAL_KEY(code) => code as u8,
    }
}

#[tracing::instrument]
pub(super) unsafe extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc called");
    let key = hwnd.0 as u32;
    defer!({
        // cleanup backend
        if msg == WM_NCDESTROY {
            trace!("cleanup hwnd: {hwnd:?}");
            BACKENDS.map.remove(&key);
        }
    });

    let mut backend = BACKENDS.map.get_mut(&key).unwrap();
    if let Some(filtered) = process_wnd_proc(&mut backend, hwnd, msg, wparam, lparam) {
        return filtered;
    }

    let original_proc = backend.original_proc;
    // prevent deadlock
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}
