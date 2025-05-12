use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode, Clone)]
pub enum InputEvent {
    Cursor(CursorEvent),
    Keyboard(KeyboardInput),
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum CursorEvent {
    Enter,
    Leave,
    Input {
        state: InputState,
        input: CursorInput,
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
    key: u8,
    state: InputState,
}

#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum CursorInput {
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
