pub mod cx;
pub mod opengl;
pub mod renderers;

use core::mem;

use asdf_overlay_common::{
    event::{ClientEvent, WindowEvent},
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
        Controls,
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, GWLP_WNDPROC, SetWindowLongPtrA, WM_NCDESTROY,
            WM_WINDOWPOSCHANGED, WNDPROC,
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

    if backend.capture_input && process_input_capture(backend, hwnd, msg, wparam, lparam) {
        return Some(LRESULT(0));
    }

    None
}

fn process_input_capture(
    backend: &mut WindowBackend,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> bool {
    match msg {
        // capture cursor input
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
        | msg::WM_NCHITTEST
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
        | msg::WM_RBUTTONDOWN
        | msg::WM_RBUTTONUP
        | msg::WM_XBUTTONDBLCLK
        | msg::WM_XBUTTONDOWN
        | msg::WM_XBUTTONUP => {}

        // capture keyboard input
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

        // capture raw input
        msg::WM_INPUT => {}

        _ => return false,
    }

    true
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
