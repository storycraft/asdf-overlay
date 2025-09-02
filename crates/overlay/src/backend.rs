pub mod render;
pub mod window;

use core::mem;
use std::collections::VecDeque;

use anyhow::Context;
use asdf_overlay_event::{OverlayEvent, WindowEvent};
use dashmap::mapref::multiple::RefMulti;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tracing::trace;
use window::proc::hooked_wnd_proc;
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    Graphics::Dxgi::IDXGIAdapter,
    UI::{
        Input::{
            Ime::{HIMC, ImmAssociateContext, ImmCreateContext, ImmDestroyContext},
            KeyboardAndMouse::{GetCapture, ReleaseCapture, SetFocus},
        },
        WindowsAndMessaging::{
            self as msg, ClipCursor, DefWindowProcA, GWLP_WNDPROC, GetClipCursor, GetSystemMetrics,
            PostMessageA, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SetCursor, SetWindowLongPtrA,
            ShowCursor, WNDPROC,
        },
    },
};

use crate::{
    backend::{
        render::RenderData,
        window::{InputBlockData, WindowProcData, cursor::load_cursor},
    },
    event_sink::OverlayEventSink,
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
    pub fn with_backend<R>(id: u32, f: impl FnOnce(&WindowBackend) -> R) -> Option<R> {
        Some(f(&*BACKENDS.map.get(&id)?))
    }

    pub fn with_or_init_backend<R>(
        id: u32,
        adapter_fn: impl FnOnce() -> Option<IDXGIAdapter>,
        f: impl FnOnce(&WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        if let Some(backend) = BACKENDS.map.get(&id) {
            return Ok(f(&backend));
        }

        let backend = BACKENDS
            .map
            .entry(id)
            .or_try_insert_with(|| {
                let original_proc: WNDPROC = unsafe {
                    mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                        HWND(id as _),
                        GWLP_WNDPROC,
                        hooked_wnd_proc as usize as _,
                    ) as _)
                };

                let interop = DxInterop::create(adapter_fn().as_ref())
                    .context("failed to create backend interop dxdevice")?;

                let window_size = get_client_size(HWND(id as _))?;

                OverlayEventSink::emit(OverlayEvent::Window {
                    id,
                    event: WindowEvent::Added {
                        width: window_size.0,
                        height: window_size.1,
                        gpu_id: interop.gpu_id(),
                    },
                });

                Ok::<_, anyhow::Error>(WindowBackend {
                    hwnd: id,
                    original_proc,
                    proc: Mutex::new(WindowProcData::new()),
                    render: Mutex::new(RenderData::new(interop, window_size)),
                    proc_queue: Mutex::new(VecDeque::new()),
                })
            })?
            .downgrade();

        Ok(f(&backend))
    }

    fn remove_backend(hwnd: HWND) {
        let key = hwnd.0 as u32;
        BACKENDS.map.remove(&key);

        OverlayEventSink::emit(OverlayEvent::Window {
            id: key,
            event: WindowEvent::Destroyed,
        });
    }

    pub fn cleanup_backends() {
        for backend in BACKENDS.map.iter() {
            backend.reset();
        }
    }
}

pub type ProcDispatchFn = Box<dyn FnOnce(&WindowBackend) + Send>;

pub struct WindowBackend {
    pub hwnd: u32,
    pub(crate) original_proc: WNDPROC,
    pub proc: Mutex<WindowProcData>,
    pub render: Mutex<RenderData>,
    pub(crate) proc_queue: Mutex<VecDeque<ProcDispatchFn>>,
}

impl WindowBackend {
    #[tracing::instrument(skip(self))]
    pub fn reset(&self) {
        trace!("backend hwnd: {:?} reset", HWND(self.hwnd as _));
        self.render.lock().reset();
        self.proc.lock().reset();
        self.block_input(false);
    }

    pub fn recalc_position(&self) {
        let mut proc = self.proc.lock();
        let mut render = self.render.lock();
        let position = proc.layout.calc(
            render
                .surface
                .get()
                .map(|surface| surface.size())
                .unwrap_or((0, 0)),
            render.window_size,
        );

        proc.position = position;
        render.position = position;
    }

    pub fn execute_gui(&self, f: impl FnOnce(&WindowBackend) + Send + 'static) {
        let mut proc_queue = self.proc_queue.lock();
        proc_queue.push_back(Box::new(f));
        unsafe {
            _ = PostMessageA(
                Some(HWND(self.hwnd as _)),
                msg::WM_NULL,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    pub fn block_input(&self, block: bool) {
        if block == self.proc.lock().blocking_state.is_some() {
            return;
        }

        if block {
            self.execute_gui(|backend| unsafe {
                if backend.proc.lock().blocking_state.is_some() {
                    return;
                }

                ShowCursor(true);
                SetCursor(backend.proc.lock().blocking_cursor.and_then(load_cursor));
                let clip_cursor = {
                    let mut rect = RECT::default();
                    _ = GetClipCursor(&mut rect);
                    let screen = RECT {
                        left: 0,
                        top: 0,
                        right: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                        bottom: GetSystemMetrics(SM_CYVIRTUALSCREEN),
                    };
                    _ = ClipCursor(None);

                    if rect != screen { Some(rect) } else { None }
                };

                let old_ime_cx =
                    ImmAssociateContext(HWND(backend.hwnd as _), ImmCreateContext()).0 as usize;

                // give focus to target window
                _ = SetFocus(Some(HWND(backend.hwnd as _)));

                // In case of ime is already enabled, hide composition windows
                DefWindowProcA(
                    HWND(backend.hwnd as _),
                    msg::WM_IME_SETCONTEXT,
                    WPARAM(1),
                    LPARAM(0),
                );
                backend.proc.lock().blocking_state = Some(InputBlockData {
                    clip_cursor,
                    old_ime_cx,
                });
            });
        } else {
            self.execute_gui(|backend| unsafe {
                ShowCursor(false);
                if GetCapture().0 as u32 == backend.hwnd {
                    _ = ReleaseCapture();
                }

                let Some(data) = backend.proc.lock().blocking_state.take() else {
                    return;
                };
                _ = ClipCursor(data.clip_cursor.as_ref().map(|r| r as _));
                let ime_cx =
                    ImmAssociateContext(HWND(backend.hwnd as _), HIMC(data.old_ime_cx as _));
                _ = ImmDestroyContext(ime_cx);

                OverlayEventSink::emit(OverlayEvent::Window {
                    id: backend.hwnd,
                    event: WindowEvent::InputBlockingEnded,
                });
            });
        }
    }
}
