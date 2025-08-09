use bincode::{
    BorrowDecode, Decode, Encode,
    de::{BorrowDecoder, Decoder},
    enc::Encoder,
    error::{DecodeError, EncodeError},
};

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
    Enabled {
        lang: String,
        conversion: ConversionMode,
    },
    /// IME changed
    Changed(String),
    /// IME conversion mode changed
    ConversionChanged(ConversionMode),
    /// IME is composing text
    Compose {
        text: String,
        caret: usize,
    },
    /// IME commit finished text
    Commit(String),
    Disabled,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConversionMode: u16 {
        const NATIVE = 1;
        const FULLSHAPE = 1 << 1;
        const NO_CONVERSION = 1 << 2;
        const HANJA_CONVERT = 1 << 3;
        const KATAKANA = 1 << 4;
    }
}

impl Encode for ConversionMode {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.bits().encode(encoder)
    }
}

impl<'de, Context> BorrowDecode<'de, Context> for ConversionMode {
    fn borrow_decode<D: BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self::from_bits_retain(u16::borrow_decode(decoder)?))
    }
}

impl<Context> Decode<Context> for ConversionMode {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self::from_bits_retain(u16::decode(decoder)?))
    }
}
