pub mod cx;
mod proc;
pub mod renderers;

use core::mem;

use anyhow::bail;
use asdf_overlay_common::{
    cursor::Cursor,
    event::{
        ClientEvent, WindowEvent,
        input::{CursorEvent, CursorInput, InputEvent},
    },
    request::UpdateSharedHandle,
};
use cx::DrawContext;
use dashmap::mapref::multiple::{RefMulti, RefMutMulti};
use once_cell::sync::Lazy;
use proc::{call_wnd_proc_hook, hooked_wnd_proc};
use renderers::Renderer;
use tracing::trace;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GWLP_WNDPROC, GetWindowThreadProcessId, SetWindowLongPtrA, SetWindowsHookExW,
        WH_GETMESSAGE, WNDPROC,
    },
};

use crate::{app::Overlay, types::IntDashMap, util::get_client_size};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: IntDashMap::default(),
    thread_hook_map: IntDashMap::default(),
});

pub struct Backends {
    map: IntDashMap<u32, WindowBackend>,
    thread_hook_map: IntDashMap<u32, usize>,
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
        let key = hwnd.0 as u32;

        let mut backend = if let Some(backend) = BACKENDS.map.get_mut(&key) {
            backend
        } else {
            let hwnd_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
            if hwnd_thread == 0 {
                bail!("GetWindowThreadProcessId failed");
            }

            BACKENDS
                .thread_hook_map
                .entry(hwnd_thread)
                .or_try_insert_with(|| unsafe {
                    SetWindowsHookExW(WH_GETMESSAGE, Some(call_wnd_proc_hook), None, hwnd_thread)
                        .map(|res| res.0 as usize)
                })?;

            let original_proc: WNDPROC = unsafe {
                mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                    hwnd,
                    GWLP_WNDPROC,
                    hooked_wnd_proc as usize as _,
                ) as _)
            };

            let size = get_client_size(hwnd)?;

            Overlay::emit_event(ClientEvent::Window {
                hwnd: key,
                event: WindowEvent::Added {
                    width: size.0,
                    height: size.1,
                },
            });

            BACKENDS.map.entry(key).insert(WindowBackend {
                hwnd: key,
                original_proc,

                listen_input: ListenInputFlags::empty(),
                blocking_state: BlockingState::None,
                blocking_cursor: Some(Cursor::Default),

                cursor_state: CursorState::Outside,

                pending_handle: None,
                size,
                renderer: Renderer::new(),
                cx: DrawContext::new(),
            })
        };

        Ok(f(&mut backend))
    }

    fn remove_backend(hwnd: HWND) {
        let key = hwnd.0 as u32;
        BACKENDS.map.remove(&key);

        Overlay::emit_event(ClientEvent::Window {
            hwnd: key,
            event: WindowEvent::Destroyed,
        });
    }

    pub fn cleanup_backends() {
        for mut backend in BACKENDS.map.iter_mut() {
            backend.cleanup();
        }
    }
}

pub struct WindowBackend {
    hwnd: u32,
    original_proc: WNDPROC,

    pub listen_input: ListenInputFlags,
    blocking_state: BlockingState,
    pub blocking_cursor: Option<Cursor>,

    cursor_state: CursorState,

    pub size: (u32, u32),
    pub pending_handle: Option<UpdateSharedHandle>,
    pub renderer: Renderer,
    pub cx: DrawContext,
}

impl WindowBackend {
    #[tracing::instrument(skip(self))]
    fn cleanup(&mut self) {
        trace!("backend hwnd: {:?} cleanup", HWND(self.hwnd as _));
        mem::take(&mut self.cx);
        mem::take(&mut self.renderer);
        self.pending_handle.take();
        self.listen_input = ListenInputFlags::empty();
        self.blocking_state.change(false);
        self.blocking_cursor = Some(Cursor::Default);
    }

    #[inline]
    fn listening_cursor(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::CURSOR)
            || self.blocking_state.is_input_blocking()
    }

    #[inline]
    fn listening_keyboard(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::KEYBOARD)
            || self.blocking_state.is_input_blocking()
    }

    pub fn block_input(&mut self, block: bool) {
        if !self.blocking_state.change(block) {
            return;
        }

        if !block {
            if let CursorState::Inside(x, y) = self.cursor_state {
                self.cursor_state = CursorState::Outside;

                Overlay::emit_event(ClientEvent::Window {
                    hwnd: self.hwnd,
                    event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                        event: CursorEvent::Leave,
                        x,
                        y,
                    })),
                });
            }

            Overlay::emit_event(ClientEvent::Window {
                hwnd: self.hwnd,
                event: WindowEvent::InputBlockingEnded,
            });

            self.blocking_cursor = Some(Cursor::Default);
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
    fn is_input_blocking(self) -> bool {
        matches!(self, Self::StartBlocking | Self::Blocking)
    }

    /// Change blocking state
    fn change(&mut self, blocking: bool) -> bool {
        if self.is_input_blocking() == blocking {
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
