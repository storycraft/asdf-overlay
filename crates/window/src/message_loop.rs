pub(crate) mod hook;

use std::collections::vec_deque::VecDeque;

use parking_lot::{Mutex, RwLock};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{PostThreadMessageA, SetCursor, ShowCursor, WM_NULL},
};

use crate::{Backends, cursors};

pub type ProcDispatchFn = Box<dyn FnOnce(&MessageLoopState) + Send>;

pub(crate) struct MessageLoopState {
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
        self.call_on_message_loop(|this| unsafe {
            ShowCursor(true);
            SetCursor(
                Backends::get()
                    .blocking_cursor()
                    .and_then(cursors::load),
            );

            *this.blocking_state.write() = Some(InputBlockingState {});
        });
    }

    pub(crate) fn unblock_input(&self) {
        *self.blocking_state.write() = None;
    }

    /// Execute a closure on the message loop thread.
    /// Calling `call_on_message_loop` inside the closure deadlock.
    pub(crate) fn call_on_message_loop(&self, f: impl FnOnce(&MessageLoopState) + Send + 'static) {
        let mut proc_queue = self.proc_queue.lock();
        if unsafe { GetCurrentThreadId() } == self.id {
            f(self);
            return;
        }

        proc_queue.push_back(Box::new(f));
        drop(proc_queue);

        // Wakeup the message loop thread.
        unsafe {
            _ = PostThreadMessageA(self.id, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

struct InputBlockingState {}
