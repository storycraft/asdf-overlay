pub mod cx;
mod proc;
pub mod renderers;

use core::mem;

use anyhow::bail;
use asdf_overlay_common::{
    event::{
        ClientEvent, WindowEvent,
        input::{CursorEvent, CursorInput, InputEvent},
    },
    key::Key,
    request::UpdateSharedHandle,
};
use bitvec::{BitArr, array::BitArray};
use cx::DrawContext;
use dashmap::mapref::multiple::{RefMulti, RefMutMulti};
use once_cell::sync::Lazy;
use proc::{call_wnd_proc_hook, hooked_wnd_proc};
use renderers::Renderer;
use tracing::trace;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GWLP_WNDPROC, GetWindowThreadProcessId, SetWindowLongPtrA, SetWindowsHookExW, ShowCursor,
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
                event: WindowEvent::Added,
            });

            BACKENDS.map.entry(key).insert(WindowBackend {
                hwnd: key,
                original_proc,

                input_capture_keybind: [None; 4],
                capturing_input: false,
                key_states: BitArray::ZERO,
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

    input_capture_keybind: [Option<Key>; 4],
    capturing_input: bool,
    key_states: BitArr!(for 512),
    cursor_state: CursorState,

    pub size: (u32, u32),
    pub pending_handle: Option<UpdateSharedHandle>,
    pub renderer: Renderer,
    pub cx: DrawContext,
}

impl WindowBackend {
    pub fn set_input_capture_keybind(&mut self, keybind: [Option<Key>; 4]) {
        self.input_capture_keybind = keybind;
        if !keybind.iter().any(|item| item.is_some()) {
            self.set_input_capture(false);
        }
    }

    #[inline]
    pub fn capturing_input(&self) -> bool {
        self.capturing_input
    }

    #[tracing::instrument(skip(self))]
    fn cleanup(&mut self) {
        trace!("backend hwnd: {:?} cleanup", HWND(self.hwnd as _));
        mem::take(&mut self.cx);
        mem::take(&mut self.renderer);
        self.pending_handle.take();
        self.input_capture_keybind = [None; 4];
        self.capturing_input = false;
    }

    fn set_input_capture(&mut self, input_capture: bool) {
        if self.capturing_input == input_capture {
            return;
        }

        if input_capture {
            Overlay::emit_event(ClientEvent::Window {
                hwnd: self.hwnd,
                event: WindowEvent::InputCaptureStart,
            });
        } else {
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
                event: WindowEvent::InputCaptureEnd,
            });
        }
        self.capturing_input = input_capture;

        // show cursor while capturing input
        // TODO: ensure ShowCursor is run on target window thread
        unsafe { ShowCursor(input_capture) };
    }

    fn update_key_state(&mut self, key: Key, value: bool) {
        #[inline]
        fn index(key: Key) -> usize {
            if key.extended {
                256 + key.code.get() as usize
            } else {
                key.code.get() as usize
            }
        }

        self.key_states.set(index(key), value);

        if !value || !self.input_capture_keybind.contains(&Some(key)) {
            return;
        }

        for keybind_key in self.input_capture_keybind {
            match keybind_key {
                Some(keybind_key) => {
                    if !self.key_states[index(keybind_key)] {
                        return;
                    }
                }
                None => continue,
            }
        }

        // toggle input capture
        self.set_input_capture(!self.capturing_input);
    }

    fn reset_key_states(&mut self) {
        self.key_states = BitArray::ZERO;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CursorState {
    Inside(i16, i16),
    Outside,
}
