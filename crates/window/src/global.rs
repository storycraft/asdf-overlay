pub(crate) mod hook;

use core::{
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use parking_lot::RwLock;
use windows::Win32::{
    Foundation::RECT,
    UI::WindowsAndMessaging::{GetSystemMetrics, HCURSOR, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN},
};

use crate::{
    cursors::default_cursor,
    event::{Event, WindowEvent},
    message_loop::MessageLoopState,
    types::IntDashMap,
    window::WindowProcState,
};

pub(crate) struct GlobalState {
    event_tx: flume::Sender<Event>,

    pub(crate) message_loops: IntDashMap<u32, MessageLoopState>,
    pub(crate) windows: IntDashMap<u32, WindowProcState>,

    blocking_cursor: AtomicUsize,
    pub(crate) blocking_state: RwLock<Option<InputBlockingState>>,
}

impl GlobalState {
    pub(crate) fn new(event_tx: flume::Sender<Event>) -> Self {
        Self {
            event_tx,
            message_loops: IntDashMap::default(),
            windows: IntDashMap::default(),
            blocking_cursor: AtomicUsize::new(default_cursor().0 as usize),
            blocking_state: RwLock::new(None),
        }
    }

    #[inline]
    pub(crate) fn blocking_cursor(&self) -> Option<HCURSOR> {
        let v = self.blocking_cursor.load(Ordering::Relaxed);
        if v == 0 { None } else { Some(HCURSOR(v as _)) }
    }

    pub(crate) fn set_blocking_cursor(&self, cursor: Option<HCURSOR>) {
        self.blocking_cursor
            .store(cursor.unwrap_or_default().0 as _, Ordering::Relaxed);
    }

    /// Check if input is currently blocked.
    #[inline]
    pub fn input_blocked(&self) -> bool {
        self.blocking_state.read().is_some()
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
        self.emit(Event::InputBlockingEnded);
    }

    /// Get or initialize the message loop state for the given thread ID.
    ///
    /// NOTE: The thread id is windows system thread id, not rust thread id.
    pub(crate) fn message_loop_state<R>(
        &self,
        thread_id: u32,
        f: impl FnOnce(&MessageLoopState) -> R,
    ) -> R {
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

    pub(crate) fn cleanup_message_loop(&self, thread_id: u32) {
        self.message_loops.remove(&thread_id);
    }

    pub(crate) fn window_state<R>(
        &self,
        window_id: u32,
        f: impl FnOnce(&WindowProcState) -> R,
    ) -> R {
        if let Some(state) = self.windows.get(&window_id) {
            return f(state.value());
        }

        let state = self
            .windows
            .entry(window_id)
            .or_try_insert_with(|| -> anyhow::Result<WindowProcState> {
                let state = WindowProcState::init(window_id)?;

                let (width, height) = state.size();
                self.emit(Event::Window {
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

    pub(crate) fn cleanup_window(&self, window_id: u32) {
        if self.windows.remove(&window_id).is_none() {
            return;
        }

        self.emit(Event::Window {
            id: window_id,
            event: WindowEvent::Destroyed,
        });
    }

    /// Emit [`BackendEvent`] to event sink. If one exists.
    #[inline]
    pub(crate) fn emit(&self, event: Event) {
        _ = self.event_tx.send(event);
    }
}

pub(crate) struct InputBlockingState {
    // Old cursor clipping rectangle, if any.
    pub(crate) clip_cursor: Option<RECT>,
}
