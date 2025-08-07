pub mod render;
pub mod window;

use core::mem;

use anyhow::Context;
use asdf_overlay_common::event::{ClientEvent, WindowEvent};
use dashmap::mapref::multiple::RefMulti;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tracing::trace;
use window::proc::hooked_wnd_proc;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrA, WNDPROC},
};

use crate::{
    app::OverlayIpc,
    backend::{render::RenderData, window::WindowProcData},
    interop::DxInterop,
    types::IntDashMap,
    util::get_client_size,
};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: IntDashMap::default(),
});

pub struct Backends {
    map: IntDashMap<u32, WindowBackend>,
}

impl Backends {
    pub fn iter<'a>() -> impl Iterator<Item = RefMulti<'a, u32, WindowBackend>> {
        BACKENDS.map.iter()
    }

    #[must_use]
    pub fn with_backend<R>(hwnd: HWND, f: impl FnOnce(&WindowBackend) -> R) -> Option<R> {
        Some(f(&*BACKENDS.map.get(&(hwnd.0 as u32))?))
    }

    pub fn with_or_init_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let key = hwnd.0 as u32;
        if let Some(backend) = BACKENDS.map.get(&key) {
            return Ok(f(&backend));
        }

        let backend = BACKENDS
            .map
            .entry(key)
            .or_try_insert_with(|| {
                let original_proc: WNDPROC = unsafe {
                    mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                        hwnd,
                        GWLP_WNDPROC,
                        hooked_wnd_proc as usize as _,
                    ) as _)
                };

                let interop =
                    DxInterop::create(None).context("failed to create backend interop dxdevice")?;

                let window_size = get_client_size(hwnd)?;

                OverlayIpc::emit_event(ClientEvent::Window {
                    hwnd: key,
                    event: WindowEvent::Added {
                        width: window_size.0,
                        height: window_size.1,
                    },
                });

                Ok::<_, anyhow::Error>(WindowBackend {
                    hwnd: key,
                    original_proc,
                    proc: Mutex::new(WindowProcData::new()),
                    render: Mutex::new(RenderData::new(interop, window_size)),
                })
            })?
            .downgrade();

        Ok(f(&backend))
    }

    fn remove_backend(hwnd: HWND) {
        let key = hwnd.0 as u32;
        BACKENDS.map.remove(&key);

        OverlayIpc::emit_event(ClientEvent::Window {
            hwnd: key,
            event: WindowEvent::Destroyed,
        });
    }

    pub fn cleanup_backends() {
        for backend in BACKENDS.map.iter() {
            backend.reset();
        }
    }
}

#[non_exhaustive]
pub struct WindowBackend {
    pub hwnd: u32,
    pub original_proc: WNDPROC,
    pub proc: Mutex<WindowProcData>,
    pub render: Mutex<RenderData>,
}

impl WindowBackend {
    #[tracing::instrument(skip(self))]
    fn reset(&self) {
        trace!("backend hwnd: {:?} reset", HWND(self.hwnd as _));
        self.proc.lock().reset();
    }
}
