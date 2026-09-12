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

/// Access to the process-wide input backend; dropping it ends input blocking.
pub struct Backends {}

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
        Ok(Self {})
    }

    /// Returns an iterator over the IDs of all windows.
    pub fn windows(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().windows.iter().map(|r| *r.key())
    }

    /// Run the closure under a read guard for an already tracked window.
    ///
    /// Returns its result in `Some`, or `None` without calling it for an unknown
    /// ID. An otherwise valid HWND may not have been observed by the hooks yet.
    /// Do not mutate the registry from the closure; this can deadlock.
    pub fn window<R>(&self, id: u32, f: impl FnOnce(&WindowProcState) -> R) -> Option<R> {
        Self::get().windows.view(&id, |_, state| f(state))
    }

    /// Returns an iterator over the IDs of all message loops.
    pub fn message_loops(&self) -> impl Iterator<Item = u32> + '_ {
        Self::get().message_loops.iter().map(|r| *r.key())
    }

    /// Run the closure under a read guard for a tracked Windows thread ID.
    ///
    /// This takes an OS thread ID, not a window ID or Rust thread ID. Unknown IDs
    /// return `None` without running the closure; known IDs return its result in
    /// `Some`. Avoid registry mutation from the closure, which can deadlock.
    pub fn message_loop<R>(&self, id: u32, f: impl FnOnce(&MessageLoopState) -> R) -> Option<R> {
        Self::get().message_loops.view(&id, |_, state| f(state))
    }

    /// Returns true if input is currently blocked.
    #[inline]
    pub fn input_blocked() -> bool {
        Self::get().input_blocked()
    }

    /// Enable input blocking across this process's intercepted windows.
    ///
    /// Repeated calls while blocked do nothing. Cursor and IME adjustments are
    /// queued to message-loop threads, so they may finish after this returns.
    #[inline]
    pub fn block_input(&self) {
        Self::get().block_input();
    }

    /// End process-wide blocking and emit an input-blocking-ended event.
    ///
    /// Does nothing if already unblocked. Cursor and IME restoration is queued
    /// and may finish after this returns.
    #[inline]
    pub fn unblock_input(&self) {
        Self::get().unblock_input();
    }

    /// Set the cursor used during input blocking, or hide it with `None`.
    ///
    /// The handle is borrowed, not copied or destroyed; keep it valid while in use.
    /// This stores the choice without waiting for a message-loop cursor update.
    #[inline]
    pub fn set_blocking_cursor(&self, cursor: Option<HCURSOR>) {
        Self::get().set_blocking_cursor(cursor);
    }

    /// Unblock input, clear all window listening flags, and restore the default cursor.
    ///
    /// Tracked windows and hooks remain installed; queued restoration can outlive
    /// this call.
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
