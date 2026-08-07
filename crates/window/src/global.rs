mod hook;

use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use asdf_overlay_common::cursor::Cursor;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use windows::Win32::Foundation::RECT;

use crate::global::hook::hook;

static GLOBAL: OnceCell<Inner> = OnceCell::new();

pub struct GlobalInputManager {
    _ph: PhantomData<()>,
}

impl GlobalInputManager {
    pub fn new() -> anyhow::Result<Self> {
        fn init_inner() -> anyhow::Result<Inner> {
            hook()?;

            Ok(Inner {
                blocking_state: RwLock::new(None),
                blocking_cursor: RwLock::new(Some(Cursor::Default)),
            })
        }

        static INITIALIZED: AtomicBool = AtomicBool::new(false);
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            panic!("GlobalInputManager can only be initialized once");
        }

        GLOBAL
            .get_or_try_init(init_inner)
            .context("initialization failed")?;
        Ok(Self { _ph: PhantomData })
    }

    pub(crate) fn get() -> &'static Inner {
        GLOBAL.get().expect("GlobalInputManager is not initialized")
    }

    /// Returns true if input is currently blocked.
    pub fn input_blocked() -> bool {
        Self::get().input_blocked()
    }

    /// Blocks or unblocks input for the window.
    pub fn block_input(&self, block: bool) {
        *Self::get().blocking_state.write() = if block {
            Some(InputBlockingState::new())
        } else {
            None
        };
    }

    /// Sets the cursor to be displayed while input is blocked.
    pub fn set_blocking_cursor(&self, cursor: Option<Cursor>) {
        *Self::get().blocking_cursor.write() = cursor;
    }
}

impl Drop for GlobalInputManager {
    fn drop(&mut self) {
        // Release input blocking on drop.
        Self::get().blocking_state.write().take();
    }
}

pub(crate) struct Inner {
    blocking_cursor: RwLock<Option<Cursor>>,
    blocking_state: RwLock<Option<InputBlockingState>>,
}

impl Inner {
    pub(crate) fn input_blocked(&self) -> bool {
        self.blocking_state.read().is_some()
    }

    pub(crate) fn blocking_cursor(&self) -> Option<Cursor> {
        self.blocking_cursor.read().clone()
    }
}

struct InputBlockingState {
    // Old cursor clipping rectangle, if any.
    clip_cursor: Option<RECT>,
}

impl InputBlockingState {
    const fn new() -> Self {
        Self { clip_cursor: None }
    }
}
