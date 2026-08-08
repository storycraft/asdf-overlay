pub(crate) struct WindowProcState {
    pub(crate) position: (i32, i32),

    pub(crate) listen_input: ListenInputFlags,
    blocking_state: Option<InputBlockData>,
    blocking_ime_cx: usize,

    cursor_state: CursorState,
    ime: ImeState,
    last_click_time: i32,
}

impl WindowProcState {
    pub fn new(blocking_ime_cx: usize) -> Self {
        Self {
            position: (0, 0),

            listen_input: ListenInputFlags::empty(),
            blocking_state: None,
            blocking_ime_cx,

            cursor_state: CursorState::Outside,
            ime: ImeState::Disabled,
            last_click_time: 0,
        }
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
struct InputBlockData {
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
    /// Flags for listening to input events.
    pub struct ListenInputFlags: u8 {
        /// Listen for cursor events.
        const CURSOR = 0b00000001;
        /// Listen for keyboard events.
        const KEYBOARD = 0b00000010;
    }
}
