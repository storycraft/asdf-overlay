pub mod cx;
pub mod opengl;
pub mod renderers;
mod wndproc;

use core::{
    mem,
    num::{NonZeroU8, NonZeroU32},
};

use asdf_overlay_common::{
    event::{ClientEvent, WindowEvent},
    request::UpdateSharedHandle,
};
use bitvec::{BitArr, array::BitArray};
use cx::DrawContext;
use dashmap::{
    DashMap,
    mapref::multiple::{RefMulti, RefMutMulti},
};
use once_cell::sync::Lazy;
use renderers::Renderer;
use rustc_hash::FxBuildHasher;
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GWLP_WNDPROC, SetWindowLongPtrA, WNDPROC},
};
use wndproc::hooked_wnd_proc;

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
                hwnd: hwnd.0 as _,
                original_proc,

                input_capture_keybind: None,
                capturing_input: false,
                key_states: BitArray::ZERO,

                pending_handle: None,
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
            backend.input_capture_keybind = None;
            backend.capturing_input = false;
        }
    }
}

pub struct WindowBackend {
    hwnd: u32,
    original_proc: WNDPROC,

    input_capture_keybind: Option<NonZeroU32>,
    capturing_input: bool,
    key_states: BitArr!(for 256),

    pub size: (u32, u32),
    pub pending_handle: Option<UpdateSharedHandle>,
    pub renderer: Renderer,
    pub cx: DrawContext,
}

impl WindowBackend {
    pub fn set_input_capture_keybind(&mut self, keybind: Option<NonZeroU32>) {
        self.input_capture_keybind = keybind;

        if keybind.is_none() {
            self.set_input_capture(false);
        }
    }

    #[inline]
    pub fn capturing_input(&self) -> bool {
        self.capturing_input
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
            Overlay::emit_event(ClientEvent::Window {
                hwnd: self.hwnd,
                event: WindowEvent::InputCaptureEnd,
            });
        }
        self.capturing_input = input_capture;
    }

    fn update_key_state(&mut self, key: NonZeroU8, value: bool) {
        self.key_states.set(key.get() as _, value);
        if self.input_capture_keybind.is_some() {
            let keybind = bytemuck::cast::<_, [u8; 4]>(self.input_capture_keybind);

            if !value || !keybind.contains(&key.get()) {
                return;
            }

            for key in keybind {
                if key == 0 {
                    continue;
                }

                if !self.key_states[key as usize] {
                    return;
                }
            }

            // toggle input capture
            self.set_input_capture(!self.capturing_input);
        }
    }
}
