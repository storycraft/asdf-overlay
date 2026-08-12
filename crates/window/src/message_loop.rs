pub(crate) mod hook;

use std::collections::vec_deque::VecDeque;

use parking_lot::{Mutex, RwLock};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{HCURSOR, PostThreadMessageA, SetCursor, ShowCursor, WM_NULL},
};

use crate::Backends;

pub type ProcDispatchFn = Box<dyn FnOnce(&MessageLoopState) + Send>;

pub struct MessageLoopState {
    /// Thread id of message loop.
    id: u32,

    blocking_state: RwLock<Option<InputBlockingState>>,
    proc_queue: Mutex<VecDeque<ProcDispatchFn>>,
}

impl MessageLoopState {
    pub(crate) fn new(id: u32) -> Self {
        Self {
            id,
            blocking_state: RwLock::new(None),
            proc_queue: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) fn block_input(&self) {
        self.spawn_fn(|this| unsafe {
            let mut blocking_state = this.blocking_state.write();
            if blocking_state.is_some() {
                return;
            }

            ShowCursor(true);
            let prev_cursor = SetCursor(Backends::get().blocking_cursor()).0 as usize;

            *blocking_state = Some(InputBlockingState { prev_cursor });
        });
    }

    pub(crate) fn unblock_input(&self) {
        self.spawn_fn(|this| unsafe {
            let Some(blocking_state) = this.blocking_state.write().take() else {
                return;
            };

            ShowCursor(false);
            SetCursor(Some(HCURSOR(blocking_state.prev_cursor as _)));
        });
    }

    /// Execute a closure on the message loop thread.
    /// Calling `call_on_message_loop` inside the closure deadlock.
    pub fn spawn_fn(&self, f: impl FnOnce(&MessageLoopState) + Send + 'static) {
        self.proc_queue.lock().push_back(Box::new(f));

        // Wakeup the message loop thread.
        unsafe {
            _ = PostThreadMessageA(self.id, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

struct InputBlockingState {
    prev_cursor: usize,
}
