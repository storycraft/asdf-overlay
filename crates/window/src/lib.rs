//! Process-wide Windows input interception and tracked window/message-loop state.
//!
//! Construct [`Backends`] once outside the loader lock. Dropping it unblocks input
//! and clears its sink, but leaves hooks installed and does not permit reinitialization.

mod event;
mod global;
pub mod message_loop;
mod types;
pub mod window;

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use anyhow::Context;
use asdf_overlay_window_event::Event;
use windows::Win32::UI::WindowsAndMessaging::HCURSOR;

use crate::{
    event::EventSink, global::GlobalState, message_loop::MessageLoopState, window::WindowProcState,
};

static GLOBAL: LazyLock<GlobalState> = LazyLock::new(GlobalState::new);

/// Process-wide input interception and tracked windows.
///
/// Initialize once, outside the loader lock. Dropping the backend unblocks input
/// and clears its event sink, but leaves hooks installed.
///
/// Input blocking captures events regardless of window listening flags. Cursor
/// and IME changes may complete after control methods return.
///
/// Registry callbacks and iterators hold read access; do not mutate the same
/// registry while using them.
pub struct Backends {
    _tmp: (),
}

impl Backends {
    /// Install process-wide input hooks and replace the window event sink.
    ///
    /// The callback runs synchronously on emitting threads, possibly concurrently
    /// and under internal locks. Queue work instead of reentering backend operations.
    /// Hook failures return an error without rolling back installed hooks or the sink.
    ///
    /// # Panics
    /// Panics on every attempt after the first, even if initialization failed or
    /// the previous backend was dropped.
    pub fn new<F>(f: F) -> anyhow::Result<Self>
    where
        F: Fn(Event) + Send + Sync + 'static,
    {
        static INITIALIZED: AtomicBool = AtomicBool::new(false);
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            panic!("GlobalInputManager can only be initialized once");
        }

        EventSink::set(f);

        asdf_overlay_hook::with_transaction(|| {
            global::hook::install().context("global win32 functions")?;
            message_loop::hook::install().context("win32 message loop functions")?;

            Ok::<_, anyhow::Error>(())
        })
        .context("hook failed")?;
        Ok(Self { _tmp: () })
    }

    /// Returns an iterator over the IDs of all windows.
    pub fn windows(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().windows.iter().map(|r| *r.key())
    }

    /// Access a tracked window, returning [`None`] if its ID is not associated to any windows.
    pub fn window<R>(&self, id: u32, f: impl FnOnce(&WindowProcState) -> R) -> Option<R> {
        Self::get().windows.view(&id, |_, state| f(state))
    }

    /// Returns an iterator over the IDs of all message loops.
    pub fn message_loops(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().message_loops.iter().map(|r| *r.key())
    }

    /// Access a tracked message loop by ID, or return [`None`] if unknown.
    pub fn message_loop<R>(&self, id: u32, f: impl FnOnce(&MessageLoopState) -> R) -> Option<R> {
        Self::get().message_loops.view(&id, |_, state| f(state))
    }

    /// Returns true if input is currently blocked.
    #[inline]
    pub fn input_blocked() -> bool {
        Self::get().input_blocked()
    }

    /// Block input to intercepted windows, doing nothing if already blocked.
    #[inline]
    pub fn block_input(&self) {
        Self::get().block_input();
    }

    /// Unblock input and emit an input-blocking-ended event.
    ///
    /// Does nothing if already unblocked.
    #[inline]
    pub fn unblock_input(&self) {
        Self::get().unblock_input();
    }

    /// Set the cursor used during input blocking, or hide it with `None`.
    ///
    /// Keep the borrowed cursor handle valid while in use.
    #[inline]
    pub fn set_blocking_cursor(&self, cursor: Option<HCURSOR>) {
        Self::get().set_blocking_cursor(cursor);
    }

    /// Unblock input, clear window listening flags, and restore the default cursor.
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
