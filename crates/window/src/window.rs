use core::{
    mem,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use scopeguard::defer;
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, SetThreadDpiAwarenessContext},
        Input::Ime::{HIMC, ImmAssociateContext, ImmCreateContext},
        WindowsAndMessaging::{
            DefWindowProcA, GWLP_WNDPROC, GetClientRect, GetWindowThreadProcessId,
            SetWindowLongPtrA, WM_IME_SETCONTEXT, WNDPROC,
        },
    },
};

use crate::{Backends, message_loop::MessageLoopState, window::proc::hooked_wnd_proc};

mod proc;

pub(crate) struct WindowProcState {
    original_proc: WNDPROC,
    id: u32,
    size: (AtomicU32, AtomicU32),

    pub(crate) input_flags: ListenInputFlags,
    blocking_state: Mutex<Option<InputBlockData>>,

    ime: RwLock<ImeState>,
    last_click_time: [Mutex<Option<Instant>>; 5],
}

impl WindowProcState {
    pub(crate) fn init(id: u32) -> anyhow::Result<Self> {
        let original_proc: WNDPROC = {
            let res = unsafe {
                mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                    HWND(id as _),
                    GWLP_WNDPROC,
                    hooked_wnd_proc as *const () as _,
                ))
            };

            if res.is_none() {
                anyhow::bail!("failed to set window proc for hwnd: {id}");
            }
            res
        };

        let size = get_client_size(HWND(id as _))?;

        Ok(Self {
            original_proc,
            id,
            size: (AtomicU32::new(size.0), AtomicU32::new(size.1)),

            input_flags: ListenInputFlags::empty(),
            blocking_state: Mutex::new(None),

            ime: RwLock::new(ImeState::Disabled),
            last_click_time: [const { Mutex::new(None) }; 5],
        })
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (
            self.size.0.load(Ordering::Relaxed),
            self.size.1.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn set_size(&self, width: u32, height: u32) {
        self.size.0.store(width, Ordering::Relaxed);
        self.size.1.store(height, Ordering::Relaxed);
    }

    pub fn update_click_time(&self, index: usize, new_time: Instant) -> Duration {
        let last_click_time = self.last_click_time[index].lock().replace(new_time);
        let Some(last_click_time) = last_click_time else {
            return Duration::from_millis(0);
        };

        new_time.duration_since(last_click_time)
    }

    pub(crate) fn block_input(&self) {
        let id = self.id;

        self.call_on_window_thread(move |_| {
            let hwnd = HWND(id as _);

            Backends::get().window_state(id, |state| {
                let mut blocking_state = state.blocking_state.lock();
                if blocking_state.is_some() {
                    return;
                }

                let old_ime_cx =
                    unsafe { ImmAssociateContext(hwnd, ImmCreateContext()) }.0 as usize;

                *blocking_state = Some(InputBlockData { old_ime_cx });
            });

            // In case of ime is already enabled, hide composition windows
            unsafe {
                DefWindowProcA(hwnd, WM_IME_SETCONTEXT, WPARAM(1), LPARAM(0));
            }
        });
    }

    pub(crate) fn unblock_input(&self) {
        let id = self.id;

        self.call_on_window_thread(move |_| {
            let hwnd = HWND(id as _);

            Backends::get().window_state(id, |state| {
                let mut blocking_state = state.blocking_state.lock();
                let Some(blocking_state) = blocking_state.take() else {
                    return;
                };

                unsafe {
                    ImmAssociateContext(hwnd, HIMC(blocking_state.old_ime_cx as _));
                }
            });
        });
    }

    pub(crate) fn call_on_window_thread(&self, f: impl FnOnce(&MessageLoopState) + Send + 'static) {
        let thread_id = unsafe { GetWindowThreadProcessId(HWND(self.id as _), None) };

        Backends::get().message_loop_state(thread_id, |message_loop| {
            message_loop.call_on_message_loop(f);
        });
    }
}

#[derive(Clone, Copy)]
struct InputBlockData {
    pub old_ime_cx: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImeState {
    Enabled,
    Compose,
    Disabled,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// Flags for listening to input events.
    pub struct ListenInputFlags: u8 {
        /// Listen for cursor events.
        const CURSOR = 0b00000001;
        /// Listen for keyboard events.
        const KEYBOARD = 0b00000010;
    }
}

/// Get DPI aware client area size of the window.
fn get_client_size(win: HWND) -> anyhow::Result<(u32, u32)> {
    unsafe {
        let old_context = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
        defer!({
            SetThreadDpiAwarenessContext(old_context);
        });

        let mut rect = RECT::default();
        GetClientRect(win, &mut rect)?;
        Ok((rect.right as u32, rect.bottom as u32))
    }
}
