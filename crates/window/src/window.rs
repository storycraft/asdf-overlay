use core::sync::atomic::{AtomicU32, Ordering};

use parking_lot::RwLock;
use windows::Win32::UI::WindowsAndMessaging::WNDPROC;

mod proc;

pub(crate) struct WindowProcState {
    size: (AtomicU32, AtomicU32),
    original_proc: WNDPROC,

    pub(crate) input_flags: ListenInputFlags,
    blocking_state: Option<InputBlockData>,

    ime: RwLock<ImeState>,
    last_click_time: i32,
}

impl WindowProcState {
    pub(crate) fn new(size: (u32, u32), original_proc: WNDPROC) -> Self {
        Self {
            size: (AtomicU32::new(size.0), AtomicU32::new(size.1)),
            original_proc,

            input_flags: ListenInputFlags::empty(),
            blocking_state: None,

            ime: RwLock::new(ImeState::Disabled),
            last_click_time: 0,
        }
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

    pub fn update_click_time(&mut self, new_time: i32) -> u32 {
        let delta = (new_time as u32).wrapping_sub(self.last_click_time as _);
        self.last_click_time = new_time;
        delta
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
