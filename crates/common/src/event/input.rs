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
        state: InputState,
        action: CursorAction,
    },
    Move,
    Scroll {
        axis: ScrollAxis,
        delta: i16,
    },
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum KeyboardInput {
    Key { key: Key, state: InputState },
    Char(char),
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
pub enum InputState {
    Pressed,
    Released,
}

#[derive(Debug, Encode, Decode, Clone, Copy)]
pub struct InputPosition {
    pub x: f32,
    pub y: f32,
}
