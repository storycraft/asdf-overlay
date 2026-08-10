mod global;
pub mod message_loop;
mod types;
pub mod window;

use core::{
    error::Error,
    fmt::Display,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use asdf_overlay_window_event::Event;
use once_cell::sync::OnceCell;
use windows::Win32::UI::WindowsAndMessaging::HCURSOR;

use crate::{global::GlobalState, message_loop::MessageLoopState, window::WindowProcState};

static GLOBAL: OnceCell<GlobalState> = OnceCell::new();

pub struct Backends {
    event_rx: flume::Receiver<Event>,
}

impl Backends {
    /// Initialize new [`Backends`] instance. This should only be called once.
    pub fn new() -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();

        let init_inner = || -> anyhow::Result<GlobalState> {
            global::hook::install()?;
            message_loop::hook::install()?;

            Ok(GlobalState::new(event_tx))
        };

        static INITIALIZED: AtomicBool = AtomicBool::new(false);
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            panic!("GlobalInputManager can only be initialized once");
        }

        GLOBAL
            .get_or_try_init(init_inner)
            .context("initialization failed")?;
        Ok(Self { event_rx })
    }

    /// Receives a [`BackendEvent`] from the backend.
    pub fn recv(&self) -> Option<Event> {
        self.event_rx.recv().ok()
    }

    /// Receives a [`BackendEvent`] from the backend.
    pub async fn recv_async(&self) -> Option<Event> {
        self.event_rx.recv_async().await.ok()
    }

    /// Tries to receive a [`BackendEvent`] from the backend.
    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        Ok(self.event_rx.try_recv()?)
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

    #[inline]
    fn get() -> &'static GlobalState {
        GLOBAL.get().expect("Backends is not initialized")
    }
}

impl Drop for Backends {
    fn drop(&mut self) {
        // Release input blocking on drop.
        Self::get().unblock_input();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

impl From<flume::TryRecvError> for TryRecvError {
    fn from(err: flume::TryRecvError) -> Self {
        match err {
            flume::TryRecvError::Empty => TryRecvError::Empty,
            flume::TryRecvError::Disconnected => TryRecvError::Disconnected,
        }
    }
}

impl Display for TryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TryRecvError::Empty => flume::TryRecvError::Empty.fmt(f),
            TryRecvError::Disconnected => flume::TryRecvError::Disconnected.fmt(f),
        }
    }
}

impl Error for TryRecvError {}
