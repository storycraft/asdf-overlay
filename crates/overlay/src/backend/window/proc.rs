use super::WindowBackend;
use crate::{
    app::OverlayIpc,
    backend::{
        BACKENDS, Backends,
        window::{BlockingState, CursorState, cursor::load_cursor},
    },
    hook::util::original_clip_cursor,
    util::get_client_size,
};
use asdf_overlay_common::event::{
    ClientEvent, WindowEvent,
    input::{
        CursorAction, CursorEvent, CursorInput, InputEvent, InputPosition, InputState, ScrollAxis,
    },
};
use core::mem;
use scopeguard::defer;
use tracing::trace;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::KeyboardAndMouse::{
            GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, GetClipCursor, SetCursor, ShowCursor, WM_NCDESTROY,
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
            let render = &mut *backend.render.lock();
            let new_size = get_client_size(HWND(backend.hwnd as _)).unwrap();
            if render.window_size != new_size {
                let proc = &mut *backend.proc.lock();
                let position = proc.layout.calc(
                    render
                        .surface
                        .get()
                        .map(|surface| surface.size())
                        .unwrap_or((0, 0)),
                    new_size,
                );

                proc.position = position;
                render.position = position;
                render.window_size = new_size;

                OverlayIpc::emit_event(ClientEvent::Window {
                    id: backend.hwnd,
                    event: WindowEvent::Resized {
                        width: new_size.0,
                        height: new_size.1,
                    },
                });
            }
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
            let mut proc = backend.proc.lock();
            if proc.input_blocking() {
                proc.block_input(false, backend.hwnd);
            }
        }

        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONDBLCLK => {
            return cursor_event::<0>(backend, CursorAction::Left, InputState::Pressed, lparam);
        }

        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONDBLCLK => {
            return cursor_event::<0>(backend, CursorAction::Middle, InputState::Pressed, lparam);
        }

        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONDBLCLK => {
            return cursor_event::<0>(backend, CursorAction::Right, InputState::Pressed, lparam);
        }

        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONDBLCLK => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);

            return cursor_event::<1>(
                backend,
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed,
                lparam,
            );
        }

        msg::WM_LBUTTONUP => {
            return cursor_event::<0>(backend, CursorAction::Left, InputState::Released, lparam);
        }
        msg::WM_MBUTTONUP => {
            return cursor_event::<0>(backend, CursorAction::Middle, InputState::Released, lparam);
        }
        msg::WM_RBUTTONUP => {
            return cursor_event::<0>(backend, CursorAction::Right, InputState::Released, lparam);
        }
        msg::WM_XBUTTONUP => {
            let [_, button] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
            return cursor_event::<1>(
                backend,
                if button == XBUTTON1 {
                    CursorAction::Back
                } else {
                    CursorAction::Forward
                },
                InputState::Pressed,
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

            match proc.blocking_state {
                BlockingState::None => {}

                BlockingState::StartBlocking => unsafe {
                    ShowCursor(true);
                    SetCursor(proc.blocking_cursor.and_then(load_cursor));
                    let mut rect = RECT::default();
                    let clip_cursor = if GetClipCursor(&mut rect).is_ok() {
                        _ = original_clip_cursor(None);
                        Some(rect)
                    } else {
                        None
                    };
                    proc.blocking_state = BlockingState::Blocking { clip_cursor };
                    return Some(LRESULT(0));
                },

                BlockingState::Blocking { .. } => return Some(LRESULT(0)),

                BlockingState::StopBlocking { clip_cursor } => unsafe {
                    ShowCursor(false);
                    if GetCapture().0 as u32 == backend.hwnd {
                        _ = ReleaseCapture();
                    }
                    if let Some(clip_cursor) = clip_cursor {
                        _ = original_clip_cursor(Some(&clip_cursor));
                    }
                    proc.blocking_state = BlockingState::None;
                },
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

        // ignore other mouse inputs
        msg::WM_POINTERUPDATE => {
            if backend.proc.lock().input_blocking() {
                return Some(LRESULT(0));
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
    backend: &WindowBackend,
    action: CursorAction,
    state: InputState,
    lparam: LPARAM,
) -> Option<LRESULT> {
    let proc = backend.proc.lock();
    if !proc.listening_cursor() {
        return None;
    }

    OverlayIpc::emit_event(cursor_input(
        backend.hwnd,
        proc.position,
        lparam,
        CursorEvent::Action { action, state },
    ));

    if proc.input_blocking() {
        match state {
            InputState::Pressed => unsafe {
                SetCapture(HWND(backend.hwnd as _));
            },
            InputState::Released => unsafe {
                _ = ReleaseCapture();
            },
        }

        Some(LRESULT(BLOCK_RESULT))
    } else {
        None
    }
}

#[inline]
fn cursor_input(
    id: u32,
    position: (i32, i32),
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
        id,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event,
            client: surface,
            window,
        })),
    }
}
