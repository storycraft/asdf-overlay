mod cursors;
pub mod event;
mod global;
mod message_loop;
mod types;
mod window;

use core::{
    error::Error,
    fmt::Display,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use asdf_overlay_common::cursor::Cursor;
use once_cell::sync::OnceCell;

use crate::{event::BackendEvent, global::GlobalState};

static GLOBAL: OnceCell<GlobalState> = OnceCell::new();

pub struct Backends {
    event_rx: flume::Receiver<BackendEvent>,
}

impl Backends {
    /// Initialize new [`Backends`] instance. This should only be called once.
    pub fn new(hinstance: usize) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();

        let init_inner = || -> anyhow::Result<GlobalState> {
            global::hook::install()?;
            message_loop::hook::install()?;

            Ok(GlobalState::new(hinstance, event_tx))
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

    pub fn recv(&self) -> Option<BackendEvent> {
        self.event_rx.recv().ok()
    }

    pub async fn recv_async(&self) -> Option<BackendEvent> {
        self.event_rx.recv_async().await.ok()
    }

    pub fn try_recv(&self) -> Result<BackendEvent, TryRecvError> {
        Ok(self.event_rx.try_recv()?)
    }

    #[inline]
    fn get() -> &'static GlobalState {
        GLOBAL.get().expect("Backends is not initialized")
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
    pub fn set_blocking_cursor(&self, cursor: Option<Cursor>) {
        *Self::get().blocking_cursor.write() = cursor;
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
