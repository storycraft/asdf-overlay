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
    pub x: i16,
    pub y: i16,
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
        delta: f32,
    },
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct KeyboardInput {
    pub key: Key,
    pub state: InputState,
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
