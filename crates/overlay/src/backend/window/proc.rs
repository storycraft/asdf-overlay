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
fn block_proc_input(
    backend: &WindowBackend,
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
            SetCursor(backend.proc.lock().blocking_cursor.and_then(load_cursor));
            return Some(LRESULT(1));
        },

        // stop input capture when user request to
        msg::WM_CLOSE => {
            backend.proc.lock().block_input(false, backend.hwnd);
        }

        msg::WM_LBUTTONDOWN | msg::WM_MBUTTONDOWN | msg::WM_RBUTTONDOWN => unsafe {
            SetCapture(HWND(backend.hwnd as _));
        },

        msg::WM_LBUTTONUP | msg::WM_MBUTTONUP | msg::WM_RBUTTONUP => unsafe {
            _ = ReleaseCapture();
        },

        // ignore mouse inputs
        Controls::WM_MOUSELEAVE
        | msg::WM_MOUSEMOVE
        | msg::WM_MOUSEWHEEL
        | msg::WM_MOUSEHWHEEL
        | msg::WM_LBUTTONDBLCLK
        | msg::WM_MBUTTONDBLCLK
        | msg::WM_RBUTTONDBLCLK => {}

        msg::WM_POINTERUPDATE => {}

        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONUP | msg::WM_XBUTTONDBLCLK => return Some(LRESULT(1)),

        _ => return None,
    }

    Some(LRESULT(0))
}

#[inline]
fn process_mouse_capture(backend: &WindowBackend, msg: u32, wparam: WPARAM, lparam: LPARAM) {
    // emit cursor action
    let emit_cursor_action = |action: CursorAction, state: InputState| {
        let proc = &mut *backend.proc.lock();

        OverlayIpc::emit_event(cursor_input(
            backend.hwnd,
            proc.position,
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
            let proc = &mut *backend.proc.lock();
            proc.cursor_state = CursorState::Outside;
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                proc.position,
                lparam,
                CursorEvent::Leave,
            ));
        }

        msg::WM_MOUSEMOVE => {
            let proc = &mut *backend.proc.lock();
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

        msg::WM_MOUSEWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            let position = backend.proc.lock().position;
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                position,
                lparam,
                CursorEvent::Scroll {
                    axis: ScrollAxis::Y,
                    delta,
                },
            ));
        }

        msg::WM_MOUSEHWHEEL => {
            let [_, delta] = bytemuck::cast::<_, [i16; 2]>(wparam.0 as u32);
            let position = backend.proc.lock().position;
            OverlayIpc::emit_event(cursor_input(
                backend.hwnd,
                position,
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

    if msg == msg::WM_WINDOWPOSCHANGED {
        let render = &mut *backend.render.lock();
        let new_size = get_client_size(hwnd).unwrap();
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
                hwnd: hwnd.0 as _,
                event: WindowEvent::Resized {
                    width: new_size.0,
                    height: new_size.1,
                },
            });
        }
    }

    if backend.proc.lock().listening_cursor() {
        // We want to skip events for non client area so listen in WndProc
        process_mouse_capture(&backend, msg, wparam, lparam);
    }

    'blocking: {
        match { backend.proc.lock().blocking_state } {
            BlockingState::None => break 'blocking,

            BlockingState::StartBlocking => unsafe {
                ShowCursor(true);
                SetCursor(backend.proc.lock().blocking_cursor.and_then(load_cursor));
                let mut rect = RECT::default();
                let clip_cursor = if GetClipCursor(&mut rect).is_ok() {
                    _ = original_clip_cursor(None);
                    Some(rect)
                } else {
                    None
                };
                backend.proc.lock().blocking_state = BlockingState::Blocking { clip_cursor };
            },

            BlockingState::Blocking { .. } => {}

            BlockingState::StopBlocking { clip_cursor } => unsafe {
                ShowCursor(false);
                if GetCapture().0 as u32 == backend.hwnd {
                    _ = ReleaseCapture();
                }
                if let Some(clip_cursor) = clip_cursor {
                    _ = original_clip_cursor(Some(&clip_cursor));
                }
                backend.proc.lock().blocking_state = BlockingState::None;
                break 'blocking;
            },
        }

        if let Some(ret) = block_proc_input(&backend, msg, wparam, lparam) {
            return ret;
        }
    }

    let original_proc = backend.original_proc;
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn cursor_input(
    hwnd: u32,
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
        hwnd,
        event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
            event,
            client: surface,
            window,
        })),
    }
}
