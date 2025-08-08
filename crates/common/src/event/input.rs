use bincode::{Decode, Encode};

use crate::key::Key;

#[derive(Debug, Encode, Decode, Clone)]
pub enum InputEvent {
    Cursor(CursorInput),
    Keyboard(KeyboardInput),
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct CursorInput {
    pub event: CursorEvent,
    /// Position relative to overlay surface
    pub client: InputPosition,
    /// Position relative to window
    pub window: InputPosition,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum CursorEvent {
    Enter,
    Leave,
    Action {
        state: CursorInputState,
        action: CursorAction,
    },
    Move,
    Scroll {
        axis: ScrollAxis,
        delta: i16,
    },
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum CursorInputState {
    Pressed {
        /// Whether if this click should be treated as part of last click of double clicking.
        double_click: bool,
    },
    Released,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum KeyboardInput {
    Key { key: Key, state: KeyInputState },
    Char(char),
    Ime(Ime),
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum CursorAction {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxis {
    X,
    Y,
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum KeyInputState {
    Pressed,
    Released,
}

#[derive(Debug, Encode, Decode, Clone, Copy)]
pub struct InputPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum Ime {
    Enabled,
    Compose { text: String, caret: usize },
    Commit(String),
    Disabled,
}
