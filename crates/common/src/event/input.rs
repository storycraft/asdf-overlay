use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode, Clone)]
pub enum InputEvent {
    Cursor(CursorInput),
    Keyboard(KeyboardInput),
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum CursorInput {
    Enter,
    Leave,
    Action {
        state: InputState,
        action: CursorAction,
        x: i16,
        y: i16,
    },
    Move {
        x: i16,
        y: i16,
    },
    Scroll {
        axis: ScrollAxis,
        delta: f32,
    },
}

#[derive(Debug, Encode, Decode, Clone)]
pub struct KeyboardInput {
    pub key: u8,
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
