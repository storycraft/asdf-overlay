mod cursors;
mod hook;
mod message_loop;
mod proc;
mod types;

use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use asdf_overlay_common::cursor::Cursor;
use asdf_overlay_event::OverlayEvent;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use windows::Win32::{
    Foundation::RECT,
    UI::{
        Input::Ime::ImmCreateContext,
        WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN},
    },
};

use crate::{message_loop::MessageLoopState, proc::WindowProcState, types::IntDashMap};

static GLOBAL: OnceCell<GlobalState> = OnceCell::new();

pub struct Backends {
    event_rx: flume::Receiver<OverlayEvent>,
}

impl Backends {
    /// Initialize new [`Backends`] instance. This should only be called once.
    pub fn new(hinstance: usize) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();

        let init_inner = || -> anyhow::Result<GlobalState> {
            hook::install()?;
            message_loop::hook::install()?;

            let blocking_ime_cx = unsafe { ImmCreateContext() }.0 as usize;
            Ok(GlobalState {
                hinstance,

                event_tx,

                message_loops: IntDashMap::default(),
                windows: IntDashMap::default(),

                blocking_state: RwLock::new(None),
                blocking_cursor: RwLock::new(Some(Cursor::Default)),
                blocking_ime_cx,
            })
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

    fn get() -> &'static GlobalState {
        GLOBAL.get().expect("Backends is not initialized")
    }

    /// Returns true if input is currently blocked.
    pub fn input_blocked() -> bool {
        Self::get().input_blocked()
    }

    /// Blocks or unblocks input for the window.
    pub fn block_input(&self) {
        let clip_cursor = {
            let mut rect = RECT::default();
            let global_hook = hook::HOOK.wait();

            unsafe {
                _ = global_hook.get_clip_cursor.original_fn()(&mut rect);
                let screen = RECT {
                    left: 0,
                    top: 0,
                    right: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    bottom: GetSystemMetrics(SM_CYVIRTUALSCREEN),
                };
                _ = global_hook.clip_cursor.original_fn()(ptr::null());

                if rect != screen { Some(rect) } else { None }
            }
        };

        let global = Self::get();
        for message_loop in global.message_loops.iter() {
            message_loop.block_input();
        }

        *global.blocking_state.write() = Some(InputBlockingState { clip_cursor });
    }

    /// Unblock input for the window.
    pub fn unblock_input(&self) {
        let global = Self::get();
        for message_loop in global.message_loops.iter() {
            message_loop.unblock_input();
        }

        *global.blocking_state.write() = None;
    }

    /// Sets the cursor to be displayed while input is blocked.
    pub fn set_blocking_cursor(&self, cursor: Option<Cursor>) {
        *Self::get().blocking_cursor.write() = cursor;
    }
}

impl Drop for Backends {
    fn drop(&mut self) {
        // Release input blocking on drop.
        Self::get().blocking_state.write().take();
    }
}

pub(crate) struct GlobalState {
    hinstance: usize,

    event_tx: flume::Sender<OverlayEvent>,

    message_loops: IntDashMap<u32, MessageLoopState>,
    windows: IntDashMap<u32, WindowProcState>,

    blocking_cursor: RwLock<Option<Cursor>>,
    blocking_state: RwLock<Option<InputBlockingState>>,
    blocking_ime_cx: usize,
}

impl GlobalState {
    pub(crate) fn input_blocked(&self) -> bool {
        self.blocking_state.read().is_some()
    }

    pub(crate) fn blocking_cursor(&self) -> Option<Cursor> {
        self.blocking_cursor.read().clone()
    }

    /// Get or initialize the message loop state for the given thread ID.
    ///
    /// NOTE: The thread id is windows system thread id, not rust thread id.
    fn message_loop_state<R>(
        &self,
        thread_id: u32,
        f: impl FnOnce(&MessageLoopState) -> R,
    ) -> R {
        match self.message_loops.get(&thread_id) {
            Some(state) => f(state.value()),

            None => f(self
                .message_loops
                .entry(thread_id)
                .or_insert_with(|| MessageLoopState::new(thread_id))
                .downgrade()
                .value()),
        }
    }

    fn cleanup_message_loop(&self, thread_id: u32) {
        self.message_loops.remove(&thread_id);
    }

    /// Emit [`OverlayEvent`] to event sink. If one exists.
    #[inline]
    pub(crate) fn emit(&self, event: OverlayEvent) {
        self.event_tx.send(event);
    }
}

struct InputBlockingState {
    // Old cursor clipping rectangle, if any.
    clip_cursor: Option<RECT>,
}
