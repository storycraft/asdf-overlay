mod click_state;
mod proc;

use core::{
    mem, slice,
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
};
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    UI::{
        Input::{
            Ime::{HIMC, ImmAssociateContext, ImmCreateContext, ImmDestroyContext},
            Touch::TOUCHINPUT,
        },
        WindowsAndMessaging::{
            DefWindowProcA, GWLP_WNDPROC, GetClientRect, GetWindowThreadProcessId,
            SetWindowLongPtrA, WM_IME_SETCONTEXT, WNDPROC,
        },
    },
};

use crate::{
    Backends,
    message_loop::MessageLoopState,
    window::{click_state::ClickState, proc::hooked_wnd_proc},
};

pub struct WindowProcState {
    original_proc: WNDPROC,
    id: u32,
    /// Thread id of the window.
    pub thread_id: u32,

    pub(crate) cursor_hovering: AtomicBool,
    size: (AtomicU32, AtomicU32),

    input_flags: AtomicU8,
    blocking_state: Mutex<Option<InputBlockData>>,

    touch_buf: Mutex<Vec<TouchInputWrap>>,

    ime: RwLock<ImeState>,
    click_state: Mutex<ClickState>,
}

impl WindowProcState {
    pub(crate) fn init(id: u32) -> anyhow::Result<Self> {
        let original_proc: WNDPROC = {
            let res = unsafe {
                mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
                    HWND(id as _),
                    GWLP_WNDPROC,
                    hooked_wnd_proc as *const () as _,
                ) as _)
            };

            if res.is_none() {
                anyhow::bail!("failed to set window proc for hwnd: {id}");
            }
            res
        };
        let thread_id = unsafe { GetWindowThreadProcessId(HWND(id as _), None) };
        let size = get_client_size(HWND(id as _))?;

        Ok(Self {
            original_proc,
            id,
            thread_id,

            cursor_hovering: AtomicBool::new(false),
            size: (AtomicU32::new(size.0), AtomicU32::new(size.1)),

            input_flags: AtomicU8::new(0),
            blocking_state: Mutex::new(None),

            touch_buf: Mutex::new(vec![]),

            ime: RwLock::new(ImeState::Disabled),
            click_state: Mutex::new(ClickState::new()),
        })
    }

    pub fn reset(&self) {
        self.input_flags.store(0, Ordering::Relaxed);
    }

    pub fn size(&self) -> (u32, u32) {
        (
            self.size.0.load(Ordering::Relaxed),
            self.size.1.load(Ordering::Relaxed),
        )
    }

    pub fn input_flags(&self) -> ListenInputFlags {
        ListenInputFlags::from_bits_retain(self.input_flags.load(Ordering::Relaxed))
    }

    pub fn set_input_flags(&self, flags: ListenInputFlags) {
        self.input_flags.store(flags.bits(), Ordering::Relaxed);
    }

    pub(crate) fn set_size(&self, width: u32, height: u32) {
        self.size.0.store(width, Ordering::Relaxed);
        self.size.1.store(height, Ordering::Relaxed);
    }

    pub(crate) fn with_touch_buf<R>(
        &self,
        count: usize,
        f: impl FnOnce(&mut [TOUCHINPUT]) -> R,
    ) -> R {
        let mut touch_buf = self.touch_buf.lock();
        touch_buf.resize(count, TouchInputWrap(TOUCHINPUT::default()));

        let len = touch_buf.len();
        f(unsafe { slice::from_raw_parts_mut(touch_buf.as_mut_ptr().cast::<TOUCHINPUT>(), len) })
    }

    pub(crate) fn get_click_count(&self, x: i32, y: i32, button: u32, new_time: Instant) -> u32 {
        self.click_state
            .lock()
            .get_click_count(x, y, button, new_time)
    }

    pub(crate) fn block_input(&self) {
        let id = self.id;

        self.spawn_fn(move |_| {
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

        self.spawn_fn(move |_| {
            let hwnd = HWND(id as _);

            Backends::get().window_state(id, |state| {
                let mut blocking_state = state.blocking_state.lock();
                let Some(blocking_state) = blocking_state.take() else {
                    return;
                };

                unsafe {
                    let ime_cx = ImmAssociateContext(hwnd, HIMC(blocking_state.old_ime_cx as _));
                    _ = ImmDestroyContext(ime_cx);
                };
            });
        });
    }

    /// Execute a closure on the window thread.
    /// Calling `call_on_window_thread` inside the closure deadlock.
    pub fn spawn_fn(&self, f: impl FnOnce(&MessageLoopState) + Send + 'static) {
        Backends::get().message_loop_state(self.thread_id, |message_loop| {
            message_loop.spawn_fn(f);
        });
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct TouchInputWrap(TOUCHINPUT);

unsafe impl Send for TouchInputWrap {}
unsafe impl Sync for TouchInputWrap {}

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

/// Get client area size of the window.
fn get_client_size(win: HWND) -> anyhow::Result<(u32, u32)> {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(win, &mut rect)?;
        Ok((rect.right as u32, rect.bottom as u32))
    }
}
