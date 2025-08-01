mod cursor;
pub mod proc;

use super::WindowBackend;
use crate::{app::OverlayIpc, layout::OverlayLayout};
use asdf_overlay_common::{
    cursor::Cursor,
    event::{
        ClientEvent, WindowEvent,
        input::{CursorEvent, CursorInput, InputEvent, InputPosition},
    },
};
use windows::Win32::Foundation::RECT;

pub struct WindowProcData {
    pub layout: OverlayLayout,
    pub position: (i32, i32),

    pub listen_input: ListenInputFlags,
    blocking_state: BlockingState,
    pub blocking_cursor: Option<Cursor>,

    cursor_state: CursorState,
}

impl WindowProcData {
    pub fn new() -> Self {
        Self {
            layout: OverlayLayout::new(),
            position: (0, 0),

            listen_input: ListenInputFlags::empty(),
            blocking_state: BlockingState::None,
            blocking_cursor: Some(Cursor::Default),

            cursor_state: CursorState::Outside,
        }
    }

    pub fn reset(&mut self) {
        self.layout = OverlayLayout::new();
        self.listen_input = ListenInputFlags::empty();
        self.blocking_state.change(false);
        self.blocking_cursor = Some(Cursor::Default);
    }

    #[inline]
    pub fn listening_cursor(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::CURSOR) || self.blocking_state.input_blocking()
    }

    #[inline]
    pub fn listening_keyboard(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::KEYBOARD)
            || self.blocking_state.input_blocking()
    }

    #[inline]
    pub fn input_blocking(&self) -> bool {
        self.blocking_state.input_blocking()
    }

    pub fn block_input(&mut self, block: bool, hwnd: u32) {
        if !self.blocking_state.change(block) {
            return;
        }

        if !block {
            if let CursorState::Inside(x, y) = self.cursor_state {
                self.cursor_state = CursorState::Outside;

                let window = InputPosition {
                    x: x as _,
                    y: y as _,
                };
                let surface = InputPosition {
                    x: window.x - self.position.0,
                    y: window.y - self.position.1,
                };
                OverlayIpc::emit_event(ClientEvent::Window {
                    id: hwnd,
                    event: WindowEvent::Input(InputEvent::Cursor(CursorInput {
                        event: CursorEvent::Leave,
                        window,
                        client: surface,
                    })),
                });
            }

            OverlayIpc::emit_event(ClientEvent::Window {
                id: hwnd,
                event: WindowEvent::InputBlockingEnded,
            });

            self.blocking_cursor = Some(Cursor::Default);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BlockingState {
    // Not Blocking
    None,

    // Start blocking, setup cursor and ime
    StartBlocking,

    // Blocking
    Blocking { clip_cursor: Option<RECT> },

    // End blocking, cleanup
    StopBlocking { clip_cursor: Option<RECT> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorState {
    Inside(i16, i16),
    Outside,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ListenInputFlags: u8 {
        const CURSOR = 0b00000001;
        const KEYBOARD = 0b00000010;
    }
}

impl BlockingState {
    #[inline]
    fn input_blocking(self) -> bool {
        matches!(self, Self::StartBlocking | Self::Blocking { .. })
    }

    /// Change blocking state
    fn change(&mut self, blocking: bool) -> bool {
        if self.input_blocking() == blocking {
            return false;
        }

        *self = match *self {
            BlockingState::None => BlockingState::StartBlocking,
            BlockingState::StartBlocking => BlockingState::None,
            // TODO
            BlockingState::Blocking { clip_cursor } => BlockingState::StopBlocking { clip_cursor },
            BlockingState::StopBlocking { clip_cursor } => BlockingState::Blocking { clip_cursor },
        };

        true
    }
}
