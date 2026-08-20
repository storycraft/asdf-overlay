mod event;
mod global;
pub mod message_loop;
mod types;
pub mod window;

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use asdf_overlay_window_event::Event;
use windows::Win32::UI::WindowsAndMessaging::HCURSOR;

use crate::{
    event::EventSink, global::GlobalState, message_loop::MessageLoopState, window::WindowProcState,
};

static GLOBAL: LazyLock<GlobalState> = LazyLock::new(GlobalState::new);

pub struct Backends {}

impl Backends {
    /// Initialize new [`Backends`] instance. This should only be called once.
    pub fn new<F>(f: F) -> anyhow::Result<Self>
    where
        F: Fn(Event) + Send + Sync + 'static,
    {
        static INITIALIZED: AtomicBool = AtomicBool::new(false);
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            panic!("GlobalInputManager can only be initialized once");
        }

        EventSink::set(f);
        global::hook::install()?;
        message_loop::hook::install()?;
        Ok(Self {})
    }

    /// Returns an iterator over the IDs of all windows.
    pub fn windows(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().windows.iter().map(|r| *r.key())
    }

    /// View the state of a window with the given ID.
    pub fn window<R>(&self, id: u32, f: impl FnOnce(&WindowProcState) -> R) -> Option<R> {
        Self::get().windows.view(&id, |_, state| f(state))
    }

    /// Returns an iterator over the IDs of all message loops.
    pub fn message_loops(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().message_loops.iter().map(|r| *r.key())
    }

    /// View the state of a message loop with the given ID.
    pub fn message_loop<R>(&self, id: u32, f: impl FnOnce(&MessageLoopState) -> R) -> Option<R> {
        Self::get().message_loops.view(&id, |_, state| f(state))
    }

    /// Returns true if input is currently blocked.
    #[inline]
    pub fn input_blocked() -> bool {
        Self::get().input_blocked()
    }

    /// Blocks or unblocks input for the window.
    #[inline]
    pub fn block_input(&self) {
        Self::get().block_input();
    }

    /// Unblock input for the window.
    #[inline]
    pub fn unblock_input(&self) {
        Self::get().unblock_input();
    }

    /// Sets the cursor to be displayed while input is blocked.
    #[inline]
    pub fn set_blocking_cursor(&self, cursor: Option<HCURSOR>) {
        Self::get().set_blocking_cursor(cursor);
    }

    pub fn reset(&self) {
        Self::get().reset();
    }

    #[inline(always)]
    fn get() -> &'static GlobalState {
        &GLOBAL
    }
}

impl Drop for Backends {
    fn drop(&mut self) {
        // Release input blocking on drop.
        Self::get().unblock_input();

        EventSink::clear();
    }
}
