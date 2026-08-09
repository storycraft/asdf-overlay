mod cursors;
pub mod event;
mod hook;
mod message_loop;
mod types;
mod window;

use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use asdf_overlay_common::cursor::Cursor;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use windows::Win32::{
    Foundation::RECT,
    UI::{
        Input::Ime::ImmCreateContext,
        WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN},
    },
};

use crate::{
    event::{BackendEvent, WindowEvent},
    message_loop::MessageLoopState,
    types::IntDashMap,
    window::WindowProcState,
};

static GLOBAL: OnceCell<GlobalState> = OnceCell::new();

pub struct Backends {
    event_rx: flume::Receiver<BackendEvent>,
}

impl Backends {
    /// Initialize new [`Backends`] instance. This should only be called once.
    pub fn new(hinstance: usize) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();

        let init_inner = || -> anyhow::Result<GlobalState> {
            hook::install()?;
            message_loop::hook::install()?;

            let _blocking_ime_cx = unsafe { ImmCreateContext() }.0 as usize;
            Ok(GlobalState {
                hinstance,

                event_tx,

                message_loops: IntDashMap::default(),
                windows: IntDashMap::default(),

                blocking_state: RwLock::new(None),
                blocking_cursor: RwLock::new(Some(Cursor::Default)),
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

pub(crate) struct GlobalState {
    hinstance: usize,

    event_tx: flume::Sender<BackendEvent>,

    message_loops: IntDashMap<u32, MessageLoopState>,
    windows: IntDashMap<u32, WindowProcState>,

    blocking_cursor: RwLock<Option<Cursor>>,
    blocking_state: RwLock<Option<InputBlockingState>>,
}

impl GlobalState {
    pub(crate) fn input_blocked(&self) -> bool {
        self.blocking_state.read().is_some()
    }

    pub(crate) fn blocking_cursor(&self) -> Option<Cursor> {
        *self.blocking_cursor.read()
    }

    /// Block input for the window.
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

        for message_loop in self.message_loops.iter() {
            message_loop.block_input();
        }

        for window in self.windows.iter() {
            window.block_input();
        }

        *self.blocking_state.write() = Some(InputBlockingState { clip_cursor });
    }

    /// Unblock input for the window.
    pub fn unblock_input(&self) {
        for message_loop in self.message_loops.iter() {
            message_loop.unblock_input();
        }

        for window in self.windows.iter() {
            window.unblock_input();
        }

        *self.blocking_state.write() = None;
        self.emit(BackendEvent::InputBlockingEnded);
    }

    /// Get or initialize the message loop state for the given thread ID.
    ///
    /// NOTE: The thread id is windows system thread id, not rust thread id.
    fn message_loop_state<R>(&self, thread_id: u32, f: impl FnOnce(&MessageLoopState) -> R) -> R {
        if let Some(state) = self.message_loops.get(&thread_id) {
            return f(state.value());
        }

        let state = self
            .message_loops
            .entry(thread_id)
            .or_insert_with(|| {
                let state = MessageLoopState::new(thread_id);
                if self.input_blocked() {
                    state.block_input();
                }

                state
            })
            .downgrade();
        f(state.value())
    }

    fn cleanup_message_loop(&self, thread_id: u32) {
        self.message_loops.remove(&thread_id);
    }

    fn window_state<R>(&self, window_id: u32, f: impl FnOnce(&WindowProcState) -> R) -> R {
        if let Some(state) = self.windows.get(&window_id) {
            return f(state.value());
        }

        let state = self
            .windows
            .entry(window_id)
            .or_try_insert_with(|| -> anyhow::Result<WindowProcState> {
                let state = WindowProcState::init(window_id)?;

                let (width, height) = state.size();
                self.emit(BackendEvent::Window {
                    id: window_id,
                    event: WindowEvent::Added { width, height },
                });

                if self.input_blocked() {
                    state.block_input();
                }

                Ok(state)
            })
            .expect("failed to initialize window state")
            .downgrade();
        f(state.value())
    }

    fn cleanup_window(&self, window_id: u32) {
        if self.windows.remove(&window_id).is_none() {
            return;
        }

        self.emit(BackendEvent::Window {
            id: window_id,
            event: WindowEvent::Destroyed,
        });
    }

    /// Emit [`BackendEvent`] to event sink. If one exists.
    #[inline]
    pub(crate) fn emit(&self, event: BackendEvent) {
        _ = self.event_tx.send(event);
    }
}

struct InputBlockingState {
    // Old cursor clipping rectangle, if any.
    clip_cursor: Option<RECT>,
}
