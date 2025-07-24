pub mod cx;
pub mod proc;
pub mod renderer;

use core::{mem, num::NonZeroU32};

use anyhow::Context;
use asdf_overlay_common::{
    cursor::Cursor,
    event::{
        ClientEvent, WindowEvent,
        input::{CursorEvent, CursorInput, InputEvent, InputPosition},
    },
    request::UpdateSharedHandle,
};
use cx::DrawContext;
use dashmap::{Entry, mapref::multiple::RefMulti};
use once_cell::sync::Lazy;
use proc::hooked_wnd_proc;
use renderer::Renderer;
use tracing::trace;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct3D11::ID3D11Device,
    UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrA, WNDPROC},
};

use crate::{
    app::OverlayIpc, interop::DxInterop, layout::OverlayLayout, surface::OverlaySurface,
    types::IntDashMap, util::get_client_size,
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
    pub fn with_backend<R>(hwnd: HWND, f: impl FnOnce(&mut WindowBackend) -> R) -> Option<R> {
        let mut backend = BACKENDS.map.get_mut(&(hwnd.0 as u32))?;
        Some(f(&mut backend))
    }

    pub fn with_or_init_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&mut WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let key = hwnd.0 as u32;
        let entry = match BACKENDS.map.entry(key) {
            Entry::Occupied(ref mut occupied) => return Ok(f(occupied.get_mut())),
            Entry::Vacant(vacant) => vacant,
        };

        let original_proc: WNDPROC = unsafe {
            mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                hwnd,
                GWLP_WNDPROC,
                hooked_wnd_proc as usize as _,
            ) as _)
        };

        let interop = DxInterop::create().context("failed to create backend interop dxdevice")?;

        let size = get_client_size(hwnd)?;

        OverlayIpc::emit_event(ClientEvent::Window {
            hwnd: key,
            event: WindowEvent::Added {
                width: size.0,
                height: size.1,
            },
        });

        Ok(f(&mut entry.insert(WindowBackend {
            hwnd: key,
            original_proc,

            interop,

            layout: OverlayLayout::new(),

            listen_input: ListenInputFlags::empty(),
            blocking_state: BlockingState::None,
            blocking_cursor: Some(Cursor::Default),

            cursor_state: CursorState::Outside,

            surface: SurfaceState::new(),
            size,
            renderer: None,
            cx: DrawContext::new(),
        })))
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
        for mut backend in BACKENDS.map.iter_mut() {
            backend.reset();
        }
    }
}

pub struct WindowBackend {
    hwnd: u32,
    original_proc: WNDPROC,

    pub interop: DxInterop,

    pub layout: OverlayLayout,

    pub listen_input: ListenInputFlags,
    blocking_state: BlockingState,
    pub blocking_cursor: Option<Cursor>,

    cursor_state: CursorState,

    pub size: (u32, u32),
    pub surface: SurfaceState,
    pub renderer: Option<Renderer>,
    pub cx: DrawContext,
}

impl WindowBackend {
    #[tracing::instrument(skip(self))]
    fn reset(&mut self) {
        trace!("backend hwnd: {:?} reset", HWND(self.hwnd as _));
        self.layout = OverlayLayout::new();
        self.surface = SurfaceState::new();
        self.listen_input = ListenInputFlags::empty();
        self.blocking_state.change(false);
        self.blocking_cursor = Some(Cursor::Default);
    }

    #[inline]
    pub fn listening_cursor(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::CURSOR) || self.blocking_state.input_blocking()
    }

    #[inline]
    pub fn listening_keyboard(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::KEYBOARD)
            || self.blocking_state.input_blocking()
    }

    #[inline]
    pub fn input_blocking(&self) -> bool {
        self.blocking_state.input_blocking()
    }

    pub fn block_input(&mut self, block: bool) {
        if !self.blocking_state.change(block) {
            return;
        }

        if !block {
            if let CursorState::Inside(x, y) = self.cursor_state {
                self.cursor_state = CursorState::Outside;

                let window = InputPosition {
                    x: x as _,
                    y: y as _,
                };
                let position = self.position();
                let surface = InputPosition {
                    x: window.x - position.0,
                    y: window.y - position.1,
                };
                OverlayIpc::emit_event(ClientEvent::Window {
                    hwnd: self.hwnd,
                    event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                        event: CursorEvent::Leave,
                        window,
                        client: surface,
                    })),
                });
            }

            OverlayIpc::emit_event(ClientEvent::Window {
                hwnd: self.hwnd,
                event: WindowEvent::InputBlockingEnded,
            });

            self.blocking_cursor = Some(Cursor::Default);
        }
    }

    pub fn update_surface(&mut self, handle: Option<NonZeroU32>) -> anyhow::Result<()> {
        self.surface.update(&self.interop.device, handle)?;
        Ok(())
    }

    pub fn set_surface_updated(&mut self) {
        self.surface.updated = true;
    }

    pub fn position(&mut self) -> (i32, i32) {
        self.layout.get_or_calc(
            self.surface
                .get()
                .map(|surface| surface.size())
                .unwrap_or((0, 0)),
            self.size,
        )
    }
}

pub struct SurfaceState {
    inner: Option<OverlaySurface>,
    updated: bool,
}

impl SurfaceState {
    const fn new() -> Self {
        Self {
            inner: None,
            updated: false,
        }
    }

    #[inline]
    pub const fn get(&self) -> Option<&OverlaySurface> {
        self.inner.as_ref()
    }

    fn update(&mut self, device: &ID3D11Device, handle: Option<NonZeroU32>) -> anyhow::Result<()> {
        self.updated = true;
        self.inner.take();

        let Some(handle) = handle else {
            return Ok(());
        };

        self.inner = Some(OverlaySurface::open_shared(device, handle.get())?);
        Ok(())
    }

    #[inline]
    pub fn take_update(&mut self) -> Option<UpdateSharedHandle> {
        if self.updated {
            self.updated = false;
            Some(UpdateSharedHandle {
                handle: self.get().map(|surface| surface.shared_handle()),
            })
        } else {
            None
        }
    }

    #[inline]
    pub fn invalidate_update(&mut self) -> bool {
        if self.updated {
            self.updated = false;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorState {
    Inside(i16, i16),
    Outside,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ListenInputFlags: u8 {
        const CURSOR = 0b00000001;
        const KEYBOARD = 0b00000010;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockingState {
    // Not Blocking
    None,

    // Start blocking, setup cursor and ime
    StartBlocking,

    // Blocking
    Blocking,

    // End blocking, cleanup
    StopBlocking,
}

impl BlockingState {
    #[inline]
    fn input_blocking(self) -> bool {
        matches!(self, Self::StartBlocking | Self::Blocking)
    }

    /// Change blocking state
    fn change(&mut self, blocking: bool) -> bool {
        if self.input_blocking() == blocking {
            return false;
        }

        *self = match self {
            BlockingState::None => BlockingState::StartBlocking,
            BlockingState::StartBlocking => BlockingState::None,
            BlockingState::Blocking => BlockingState::StopBlocking,
            BlockingState::StopBlocking => BlockingState::Blocking,
        };

        true
    }
}
