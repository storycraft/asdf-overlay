pub(crate) mod cursor;
pub(crate) mod proc;

use super::WindowBackend;
use asdf_overlay_common::cursor::Cursor;
use windows::Win32::Foundation::RECT;

pub(crate) struct WindowProcData {
    pub position: (i32, i32),

    pub listen_input: ListenInputFlags,
    pub blocking_state: Option<InputBlockData>,
    pub blocking_cursor: Option<Cursor>,

    cursor_state: CursorState,
    ime: ImeState,
    last_click_time: i32,
}

impl WindowProcData {
    pub fn new() -> Self {
        Self {
            position: (0, 0),

            listen_input: ListenInputFlags::empty(),
            blocking_state: None,
            blocking_cursor: Some(Cursor::Default),

            cursor_state: CursorState::Outside,
            ime: ImeState::Disabled,
            last_click_time: 0,
        }
    }

    pub fn reset(&mut self) {
        self.position = (0, 0);
        self.listen_input = ListenInputFlags::empty();
        self.blocking_cursor = Some(Cursor::Default);
    }

    #[inline]
    pub fn listening_cursor(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::CURSOR) || self.blocking_state.is_some()
    }

    #[inline]
    pub fn listening_keyboard(&self) -> bool {
        self.listen_input.contains(ListenInputFlags::KEYBOARD) || self.blocking_state.is_some()
    }

    #[inline]
    pub fn input_blocking(&self) -> bool {
        self.blocking_state.is_some()
    }

    pub fn update_click_time(&mut self, new_time: i32) -> u32 {
        let delta = (new_time as u32).wrapping_sub(self.last_click_time as _);
        self.last_click_time = new_time;
        delta
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InputBlockData {
    pub clip_cursor: Option<RECT>,
    pub old_ime_cx: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorState {
    Inside(i16, i16),
    Outside,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImeState {
    Enabled,
    Compose,
    Disabled,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ListenInputFlags: u8 {
        const CURSOR = 0b00000001;
        const KEYBOARD = 0b00000010;
    }
}
