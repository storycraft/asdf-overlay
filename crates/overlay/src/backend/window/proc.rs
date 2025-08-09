use super::WindowBackend;
use crate::{
    app::OverlayIpc,
    backend::{
        BACKENDS, Backends,
        window::{CursorState, ImeState, WindowProcData, cursor::load_cursor},
    },
    util::get_client_size,
};
use asdf_overlay_common::event::{
    ClientEvent, WindowEvent,
    input::{
        CursorAction, CursorEvent, CursorInput, CursorInputState, Ime, InputEvent, InputPosition,
        KeyboardInput, ScrollAxis,
    },
};
use core::mem;
use parking_lot::MutexGuard;
use scopeguard::defer;
use tracing::trace;
use utf16string::{LittleEndian, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::{
            Ime::{
                CS_NOMOVECARET, GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR, HIMC,
                IME_COMPOSITION_STRING, ISC_SHOWUICOMPOSITIONWINDOW, ImmGetCompositionStringW,
                ImmGetContext, ImmReleaseContext,
            },
            KeyboardAndMouse::{
                GetDoubleClickTime, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT,
                TrackMouseEvent,
            },
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, GetMessageTime, SetCursor, WM_NCDESTROY,
            XBUTTON1,
        },
    },
};

#[inline]
fn process_wnd_proc(
    backend: &WindowBackend,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match msg {
        msg::WM_WINDOWPOSCHANGED => {
            let new_size = get_client_size(HWND(backend.hwnd as _)).unwrap();
            let mut render = backend.render.lock();
            if render.window_size != new_size {
                render.window_size = new_size;

                OverlayIpc::emit_event(ClientEvent::Window {
                    id: backend.hwnd,
                    event: WindowEvent::Resized {
                        width: new_size.0,
                        height: new_size.1,
                    },
                });
            }
            drop(render);
            backend.recalc_position();
        }

        // set cursor in client area
        msg::WM_SETCURSOR
            if {
                let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
                // check if cursor is on client
                area == 1
            } =>
        {
            let proc = backend.proc.lock();
            if proc.input_blocking() {
                unsafe { SetCursor(proc.blocking_cursor.and_then(load_cursor)) };
                return Some(LRESULT(1));
            }
        }

        // stop input capture when user request to
        msg::WM_CLOSE => {
            let input_blocking = backend.proc.lock().input_blocking();
            if input_blocking {
                backend.block_input(false);
                return Some(LRESULT(0));
            }
        }

        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Left, state, lparam);
        }

        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Middle, state, lparam);
        }

        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<0>(backend.hwnd, proc, CursorAction::Right, state, lparam);
        }

        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONDBLCLK => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            let mut proc = backend.proc.lock();
            let state = CursorInputState::Pressed {
                double_click: check_double_click(&mut proc),
            };
            return cursor_event::<1>(
                backend.hwnd,
                proc,
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                state,
                lparam,
            );
        }

        msg::WM_LBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Left,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_MBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Middle,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_RBUTTONUP => {
            return cursor_event::<0>(
                backend.hwnd,
                backend.proc.lock(),
                CursorAction::Right,
                CursorInputState::Released,
                lparam,
            );
        }
        msg::WM_XBUTTONUP => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            return cursor_event::<1>(
                backend.hwnd,
                backend.proc.lock(),
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                CursorInputState::Released,
                lparam,
            );
        }

        Controls::WM_MOUSELEAVE => {
            let mut proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            proc.cursor_state = CursorState::Outside;
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Leave,
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_MOUSEMOVE => {
            let mut proc = backend.proc.lock();
            if proc.listening_cursor() {
                let [x, y] = bytemuck::cast::<_, [i16; 2]>(lparam.0 as u32);

                match proc.cursor_state {
                    CursorState::Inside(ref mut old_x, ref mut old_y) => {
                        *old_x = x;
                        *old_y = y;
                    }
                    CursorState::Outside => {
                        proc.cursor_state = CursorState::Inside(x, y);
                        OverlayIpc::emit_event(cursor_input(
                            backend.hwnd,
                            proc.position,
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
                    proc.position,
                    lparam,
                    CursorEvent::Move,
                ));
            }
        }

        msg::WM_MOUSEWHEEL => {
            let proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_MOUSEHWHEEL => {
            let proc = backend.proc.lock();
            if !proc.listening_cursor() {
                return None;
            }

            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::X,
                    delta,
                },
            ));

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_APPCOMMAND => {
            let input_blocking = backend.proc.lock().input_blocking();
            if input_blocking {
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        // block other keyboard, mouse event
        msg::WM_CAPTURECHANGED
        | msg::WM_ACTIVATE
        | msg::WM_SETFOCUS
        | msg::WM_KILLFOCUS
        | msg::WM_POINTERUPDATE
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR => {
            let proc = backend.proc.lock();
            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_SETCONTEXT => {
            let proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            OverlayIpc::emit_event(keyboard_input(
                backend.hwnd,
                KeyboardInput::Ime(if wparam.0 != 0 {
                    Ime::Enabled
                } else {
                    Ime::Disabled
                }),
            ));

            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(
                        HWND(backend.hwnd as _),
                        msg,
                        wparam,
                        LPARAM(lparam.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize)),
                    )
                });
            }
        }

        msg::WM_IME_STARTCOMPOSITION => {
            let mut proc = backend.proc.lock();
            proc.ime = ImeState::Enabled;
            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        msg::WM_IME_COMPOSITION => {
            let mut proc = backend.proc.lock();
            if !proc.listening_keyboard() {
                return None;
            }

            if proc.ime != ImeState::Disabled {
                let hwnd = HWND(backend.hwnd as _);
                let himc = unsafe { ImmGetContext(hwnd) };
                defer!(unsafe {
                    _ = ImmReleaseContext(hwnd, himc);
                });

                let comp = lparam.0 as u32;

                // cancelled
                if comp == 0 {
                    OverlayIpc::emit_event(keyboard_input(
                        backend.hwnd,
                        KeyboardInput::Ime(Ime::Compose {
                            text: String::new(),
                            caret: 0,
                        }),
                    ));
                }

                if comp & GCS_RESULTSTR.0 != 0 {
                    if let Some(text) = get_ime_string(himc, GCS_RESULTSTR) {
                        proc.ime = ImeState::Enabled;
                        OverlayIpc::emit_event(keyboard_input(
                            backend.hwnd,
                            KeyboardInput::Ime(Ime::Commit(text.to_utf8())),
                        ));
                    }
                }

                if comp & (GCS_COMPSTR.0 | GCS_COMPATTR.0 | GCS_CURSORPOS.0) != 0 {
                    let caret = if comp & CS_NOMOVECARET == 0 && comp & GCS_CURSORPOS.0 != 0 {
                        unsafe { ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0) as usize }
                    } else {
                        0
                    };

                    if let Some(text) = get_ime_string(himc, GCS_COMPSTR) {
                        proc.ime = ImeState::Compose;

                        OverlayIpc::emit_event(keyboard_input(
                            backend.hwnd,
                            KeyboardInput::Ime(Ime::Compose {
                                text: text.to_utf8(),
                                caret,
                            }),
                        ));
                    }
                }
            }

            if proc.input_blocking() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_ENDCOMPOSITION => {
            let mut proc = backend.proc.lock();
            let ime = proc.ime;
            proc.ime = ImeState::Disabled;

            if ime == ImeState::Compose {
                let hwnd = HWND(backend.hwnd as _);
                let himc = unsafe { ImmGetContext(hwnd) };
                defer!(unsafe {
                    _ = ImmReleaseContext(hwnd, himc);
                });
                if let Some(text) = get_ime_string(himc, GCS_RESULTSTR) {
                    OverlayIpc::emit_event(keyboard_input(
                        backend.hwnd,
                        KeyboardInput::Ime(Ime::Commit(text.to_utf8())),
                    ));
                }
            }

            if proc.input_blocking() {
                drop(proc);
                return Some(unsafe {
                    DefWindowProcA(HWND(backend.hwnd as _), msg, wparam, lparam)
                });
            }
        }

        _ => {}
    }
    None
}

#[tracing::instrument]
pub(crate) unsafe extern "system" fn hooked_wnd_proc(
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

    let backend = BACKENDS.map.get(&(hwnd.0 as u32)).unwrap();
    if let Some(ret) = process_wnd_proc(&backend, msg, wparam, lparam) {
        return ret;
    }
    let original_proc = backend.original_proc;
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn cursor_event<const BLOCK_RESULT: isize>(
    hwnd: u32,
    proc: MutexGuard<WindowProcData>,
    action: CursorAction,
    state: CursorInputState,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if !proc.listening_cursor() {
        return None;
    }

    OverlayIpc::emit_event(cursor_input(
        hwnd,
        proc.position,
        lparam,
        CursorEvent::Action { action, state },
    ));

    if proc.input_blocking() {
        // prevent deadlock
        drop(proc);
        match state {
            CursorInputState::Pressed { .. } => unsafe {
                SetCapture(HWND(hwnd as _));
            },
            CursorInputState::Released => unsafe {
                _ = ReleaseCapture();
            },
        }

        Some(LRESULT(BLOCK_RESULT))
    } else {
        None
    }
}

#[inline]
fn check_double_click(proc: &mut WindowProcData) -> bool {
    proc.update_click_time(unsafe { GetMessageTime() }) <= unsafe { GetDoubleClickTime() }
}

#[inline]
fn cursor_input(id: u32, position: (i32, i32), lparam: LPARAM, event: CursorEvent) -> ClientEvent {
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
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event,
            client: surface,
            window,
        })),
    }
}

#[inline(always)]
fn keyboard_input(id: u32, input: KeyboardInput) -> ClientEvent {
    ClientEvent::Window {
        id,
        event: WindowEvent::Input(InputEvent::Keyboard(input)),
    }
}

#[inline]
fn get_ime_string(himc: HIMC, comp: IME_COMPOSITION_STRING) -> Option<WString<LittleEndian>> {
    let byte_size = unsafe { ImmGetCompositionStringW(himc, comp, None, 0) };
    if byte_size >= 0 {
        let mut buf = vec![0_u8; byte_size as usize];

        unsafe {
            ImmGetCompositionStringW(himc, comp, Some(buf.as_mut_ptr().cast()), buf.len() as _)
        };

        WString::from_utf16le(buf).ok()
    } else {
        None
    }
}
