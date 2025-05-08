pub mod renderers;

use core::{ffi::c_void, mem, ptr};

use asdf_overlay_common::message::{ClientEvent, ResizeEvent};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use renderers::Renderers;
use rustc_hash::FxBuildHasher;
use tracing::trace;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallWindowProcA, GWLP_WNDPROC, SetWindowLongPtrA, WM_WINDOWPOSCHANGED, WNDPROC,
    },
};

use crate::{app::Overlay, util::get_client_size};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: DashMap::default(),
});

pub struct Backends {
    map: DashMap<usize, WindowBackend, FxBuildHasher>,
}

impl Backends {
    pub fn with_backend<R>(hwnd: HWND, f: impl FnOnce(&mut WindowBackend) -> R) -> Option<R> {
        let mut backend = BACKENDS.map.get_mut(&(hwnd.0 as usize))?;
        Some(f(&mut backend))
    }

    pub fn with_or_init_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&mut WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let mut backend = BACKENDS.map.entry(hwnd.0 as usize).or_try_insert_with(|| {
            let original_proc: WNDPROC = unsafe {
                mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                    hwnd,
                    GWLP_WNDPROC,
                    hooked_wnd_proc as usize as _,
                ) as _)
            };

            let size = get_client_size(hwnd)?;

            Ok::<_, anyhow::Error>(WindowBackend {
                hwnd: hwnd.0 as usize,
                original_proc,

                size,
                renderers: Renderers::new(),
            })
        })?;

        Ok(f(&mut backend))
    }

    pub fn cleanup_renderers() {
        for mut backend in BACKENDS.map.iter_mut() {
            mem::take(&mut backend.renderers);
        }
    }
}

impl Drop for WindowBackend {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrA(
                HWND(ptr::null_mut::<c_void>().with_addr(self.hwnd)),
                GWLP_WNDPROC,
                mem::transmute::<WNDPROC, isize>(self.original_proc) as _,
            )
        };
    }
}

pub struct WindowBackend {
    hwnd: usize,
    original_proc: WNDPROC,

    pub size: (u32, u32),
    pub renderers: Renderers,
}

fn process_wnd_proc(
    backend: &mut WindowBackend,
    hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> Option<LRESULT> {
    if msg == WM_WINDOWPOSCHANGED {
        backend.size = get_client_size(hwnd).unwrap();
        Overlay::emit_event(ClientEvent::Resize(ResizeEvent {
            hwnd: hwnd.0 as u32,
            width: backend.size.0,
            height: backend.size.1,
        }));
    }

    None
}

#[tracing::instrument]
extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WNDPROC called");
    let mut backend = BACKENDS.map.get_mut(&(hwnd.0 as usize)).unwrap();
    if let Some(filtered) = process_wnd_proc(&mut backend, hwnd, msg, wparam, lparam) {
        return filtered;
    }

    let original_proc = backend.original_proc;
    // prevent deadlock
    drop(backend);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}
