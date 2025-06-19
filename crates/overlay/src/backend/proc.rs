mod cursor;

use super::WindowBackend;
use crate::{
    app::OverlayIpc,
    backend::{BACKENDS, Backends, BlockingState, CursorState},
    util::get_client_size,
};
use asdf_overlay_common::{
    event::{
        ClientEvent, WindowEvent,
        input::{
            CursorAction, CursorEvent, CursorInput, InputEvent, InputPosition, InputState,
            KeyboardInput, ScrollAxis,
        },
    },
    key::Key,
};
use core::mem;
use cursor::load_cursor;
use scopeguard::defer;
use tracing::trace;
use utf16string::{LittleEndian, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::{
            Ime::{GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext},
            KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_F10, VK_MENU},
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, GA_ROOT, GetAncestor, MSG, SetCursor,
            ShowCursor, WM_NCDESTROY, XBUTTON1,
        },
    },
};

#[inline]
fn block_proc_input(
    backend: &mut WindowBackend,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match msg {
        // set cursor in client area
        msg::WM_SETCURSOR
            if {
                let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
                area == 1
            } =>
        unsafe {
            SetCursor(backend.blocking_cursor.and_then(load_cursor));
            return Some(LRESULT(1));
        },

        // stop input capture when user request to
        msg::WM_CLOSE => {
            backend.block_input(false);
        }

        // ignore mouse inputs
        msg::WM_LBUTTONDOWN
        | msg::WM_LBUTTONUP
        | msg::WM_MBUTTONDOWN
        | msg::WM_MBUTTONUP
        | msg::WM_RBUTTONDOWN
        | msg::WM_RBUTTONUP
        | Controls::WM_MOUSELEAVE
        | msg::WM_MOUSEMOVE
        | msg::WM_MOUSEWHEEL
        | msg::WM_MOUSEHWHEEL
        | msg::WM_LBUTTONDBLCLK
        | msg::WM_MBUTTONDBLCLK
        | msg::WM_RBUTTONDBLCLK => {}

        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONUP | msg::WM_XBUTTONDBLCLK => return Some(LRESULT(1)),

        // ignore raw input (ignoring in hook leak handle)
        msg::WM_INPUT => {}

        _ => return None,
    }

    Some(LRESULT(0))
}

#[inline]
fn process_mouse_capture(backend: &mut WindowBackend, msg: u32, wparam: WPARAM, lparam: LPARAM) {
    // emit cursor action
    let mut emit_cursor_action = |action: CursorAction, state: InputState| {
        OverlayIpc::emit_event(cursor_input(
            backend.hwnd,
            backend.position(),
            lparam,
            CursorEvent::Action { action, state },
        ));
    };

    match msg {
        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            emit_cursor_action(CursorAction::Left, InputState::Pressed)
        }
        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            emit_cursor_action(CursorAction::Middle, InputState::Pressed)
        }
        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            emit_cursor_action(CursorAction::Right, InputState::Pressed)
        }
        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONDBLCLK => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            emit_cursor_action(
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed,
            );
        }

        msg::WM_LBUTTONUP => emit_cursor_action(CursorAction::Left, InputState::Released),
        msg::WM_MBUTTONUP => emit_cursor_action(CursorAction::Middle, InputState::Released),
        msg::WM_RBUTTONUP => emit_cursor_action(CursorAction::Right, InputState::Released),
        msg::WM_XBUTTONUP => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            emit_cursor_action(
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed,
            );
        }

        Controls::WM_MOUSELEAVE => {
            backend.cursor_state = CursorState::Outside;
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                backend.position(),
                lparam,
                CursorEvent::Leave,
            ));
        }

        msg::WM_MOUSEMOVE => {
            let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

            match backend.cursor_state {
                CursorState::Inside(ref mut old_x, ref mut old_y) => {
                    *old_x = x;
                    *old_y = y;
                }
                CursorState::Outside => {
                    backend.cursor_state = CursorState::Inside(x, y);
                    OverlayIpc::emit_event(cursor_input(
                        backend.hwnd,
                        backend.position(),
                        lparam,
                        CursorEvent::Enter,
                    ));

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

            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                backend.position(),
                lparam,
                CursorEvent::Move,
            ));
        }

        msg::WM_MOUSEWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                backend.position(),
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));
        }

        msg::WM_MOUSEHWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                backend.position(),
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                },
            ));
        }

        _ => {}
    }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_wnd_proc(
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

    if msg == msg::WM_WINDOWPOSCHANGED {
        let new_size = get_client_size(hwnd).unwrap();
        if backend.size != new_size {
            backend.size = new_size;
            OverlayIpc::emit_event(ClientEvent::Window {
                hwnd: backend.hwnd,
                event: WindowEvent::Resized {
                    width: backend.size.0,
                    height: backend.size.1,
                },
            });
        }
    }

    if backend.listening_cursor() {
        // We want to skip events for non client area so listen in WndProc
        process_mouse_capture(&mut backend, msg, wparam, lparam);
    }

    'blocking: {
        match backend.blocking_state {
            BlockingState::None => break 'blocking,

            BlockingState::StartBlocking => unsafe {
                ShowCursor(true);
                SetCursor(backend.blocking_cursor.and_then(load_cursor));
                backend.blocking_state = BlockingState::Blocking;
            },

            BlockingState::Blocking => {}

            BlockingState::StopBlocking => unsafe {
                ShowCursor(false);
                backend.blocking_state = BlockingState::None;
                break 'blocking;
            },
        }

        if let Some(ret) = block_proc_input(&mut backend, msg, wparam, lparam) {
            return ret;
        }
    }

    let original_proc = backend.original_proc;
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn process_keyboard_listen(backend: &mut WindowBackend, msg: &MSG) -> Option<LRESULT> {
    fn emit_key_input(
        backend: &mut WindowBackend,
        msg: &MSG,
        state: InputState,
    ) -> Option<LRESULT> {
        if let Some(key) = to_key(msg.wParam, msg.lParam) {
            OverlayIpc::emit_event(keyboard_input(
                backend.hwnd,
                KeyboardInput::Key { key, state },
            ));

            if backend.input_blocking() {
                let key = key.code.get() as u16;
                // ignore f10, or menu key
                // Default proc try to open non existent menu on some app and freezes window
                if key == VK_F10.0 || key == VK_MENU.0 {
                    return None;
                }

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
        }
        msg::WM_IME_COMPOSITION if msg.lParam.0 as u32 == GCS_RESULTSTR.0 => {
            if let Some(str) = get_ime_string(HWND(backend.hwnd as _)) {
                for ch in str.chars() {
                    OverlayIpc::emit_event(keyboard_input(backend.hwnd, KeyboardInput::Char(ch)));
                }
            }
        }

        // ignore remaining keyboard inputs
        msg::WM_APPCOMMAND
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR => {}

        _ => return None,
    }

    if backend.input_blocking() {
        Some(LRESULT(0))
    } else {
        None
    }
}

pub(crate) fn dispatch_message(msg: &MSG) -> Option<LRESULT> {
    if !msg.hwnd.is_invalid() {
        let root_hwnd = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
        if let Some(ret) = Backends::with_backend(root_hwnd, |backend| {
            if backend.listening_keyboard() {
                process_keyboard_listen(backend, msg)
            } else {
                None
            }
        })
        .flatten()
        {
            return Some(ret);
        }
    }

    None
}

#[inline]
fn cursor_input(
    hwnd: u32,
    position: (f32, f32),
    lparam: LPARAM,
    event: CursorEvent,
) -> ClientEvent {
    let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

    let window = InputPosition {
        x: x as _,
        y: y as _,
    };
    let surface = InputPosition {
        x: window.x - position.0,
        y: window.y - position.1,
    };
    ClientEvent::Window {
        hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event,
            client: surface,
            window,
        })),
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
