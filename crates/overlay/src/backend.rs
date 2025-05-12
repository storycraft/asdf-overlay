pub mod cx;
pub mod opengl;
pub mod renderers;

use core::mem;

use asdf_overlay_common::{
    event::{
        ClientEvent, WindowEvent,
        input::{CursorAction, CursorInput, InputEvent, InputState, KeyboardInput, ScrollAxis},
    },
    request::UpdateSharedHandle,
};
use cx::DrawContext;
use dashmap::{
    DashMap,
    mapref::multiple::{RefMulti, RefMutMulti},
};
use once_cell::sync::Lazy;
use renderers::Renderer;
use rustc_hash::FxBuildHasher;
use scopeguard::defer;
use tracing::trace;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Controls::{self, HOVER_DEFAULT},
        Input::KeyboardAndMouse::{TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, GWLP_WNDPROC, SetWindowLongPtrA,
            WHEEL_DELTA, WM_NCDESTROY, WM_WINDOWPOSCHANGED, WNDPROC, XBUTTON1,
        },
    },
};

use crate::{app::Overlay, util::get_client_size};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: DashMap::default(),
});

pub struct Backends {
    map: DashMap<u32, WindowBackend, FxBuildHasher>,
}

impl Backends {
    pub fn iter<'a>() -> impl Iterator<Item = RefMulti<'a, u32, WindowBackend>> {
        BACKENDS.map.iter()
    }

    pub fn iter_mut<'a>() -> impl Iterator<Item = RefMutMulti<'a, u32, WindowBackend>> {
        BACKENDS.map.iter_mut()
    }

    #[must_use]
    pub fn with_backend<R>(hwnd: HWND, f: impl FnOnce(&mut WindowBackend) -> R) -> Option<R> {
        let mut backend = BACKENDS.map.get_mut(&(hwnd.0 as u32))?;
        Some(f(&mut backend))
    }

    pub fn with_or_init_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&mut WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let mut backend = BACKENDS.map.entry(hwnd.0 as u32).or_try_insert_with(|| {
            let original_proc: WNDPROC = unsafe {
                mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                    hwnd,
                    GWLP_WNDPROC,
                    hooked_wnd_proc as usize as _,
                ) as _)
            };

            let size = get_client_size(hwnd)?;

            Overlay::emit_event(ClientEvent::Window {
                hwnd: hwnd.0 as u32,
                event: WindowEvent::Added,
            });

            Ok::<_, anyhow::Error>(WindowBackend {
                original_proc,

                pending_handle: None,
                capture_input: false,
                size,
                renderer: Renderer::new(),
                cx: DrawContext::new(),
            })
        })?;

        Ok(f(&mut backend))
    }

    pub fn cleanup_renderers() {
        for mut backend in BACKENDS.map.iter_mut() {
            mem::take(&mut backend.renderer);
            backend.pending_handle.take();
            backend.capture_input = false;
        }
    }
}

pub struct WindowBackend {
    original_proc: WNDPROC,

    pub size: (u32, u32),
    pub capture_input: bool,
    pub pending_handle: Option<UpdateSharedHandle>,
    pub renderer: Renderer,
    pub cx: DrawContext,
}

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

        _ => {}
    }

    if backend.capture_input {
        if let Some(res) = process_input_capture(hwnd, msg, wparam, lparam) {
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
                    key: wparam.0 as u8,
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
        | msg::WM_NCLBUTTONDBLCLK
        | msg::WM_NCLBUTTONDOWN
        | msg::WM_NCLBUTTONUP
        | msg::WM_NCMBUTTONDBLCLK
        | msg::WM_NCMBUTTONDOWN
        | msg::WM_NCMBUTTONUP
        | msg::WM_NCMOUSEHOVER
        | msg::WM_NCMOUSELEAVE
        | msg::WM_NCMOUSEMOVE
        | msg::WM_NCRBUTTONDBLCLK
        | msg::WM_NCRBUTTONDOWN
        | msg::WM_NCRBUTTONUP
        | msg::WM_NCXBUTTONDBLCLK
        | msg::WM_NCXBUTTONDOWN
        | msg::WM_NCXBUTTONUP
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

#[tracing::instrument]
extern "system" fn hooked_wnd_proc(
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
