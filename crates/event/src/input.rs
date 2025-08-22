use core::num::NonZeroU8;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum InputEvent {
    Cursor(CursorInput),
    Keyboard(KeyboardInput),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct CursorInput {
    pub event: CursorEvent,
    /// Position relative to overlay surface
    pub client: InputPosition,
    /// Position relative to window
    pub window: InputPosition,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum CursorInputState {
    Pressed {
        /// Whether if this click should be treated as part of last click of double clicking.
        double_click: bool,
    },
    Released,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum KeyboardInput {
    Key { key: Key, state: KeyInputState },
    Char(char),
    Ime(Ime),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct Key {
    pub code: NonZeroU8,
    pub extended: bool,
}

impl Key {
    pub fn new(code: u8, extended: bool) -> Option<Self> {
        NonZeroU8::new(code).map(|code| Key { code, extended })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum CursorAction {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum ScrollAxis {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum KeyInputState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct InputPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum Ime {
    Enabled {
        lang: String,
        conversion: ConversionMode,
    },
    /// IME changed
    Changed(String),
    /// IME conversion mode changed
    ConversionChanged(ConversionMode),
    /// IME candidates are added/changed
    CandidateChanged(ImeCandidateList),
    /// IME candidates are closed
    CandidateClosed,
    /// IME is composing text
    Compose {
        text: String,
        caret: usize,
    },
    /// IME commit finished text
    Commit(String),
    Disabled,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct ImeCandidateList {
    pub page_start_index: u32,
    pub page_size: u32,
    pub selected_index: u32,
    pub candidates: Vec<String>,
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

#[cfg(feature = "bincode")]
const _: () = {
    use bincode::{
        BorrowDecode,
        de::{BorrowDecoder, Decoder},
        enc::Encoder,
        error::{DecodeError, EncodeError},
    };

    impl bincode::Encode for ConversionMode {
        fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
            self.bits().encode(encoder)
        }
    }

    impl<'de, Context> BorrowDecode<'de, Context> for ConversionMode {
        fn borrow_decode<D: BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, DecodeError> {
            Ok(Self::from_bits_retain(u16::borrow_decode(decoder)?))
        }
    }

    impl<Context> bincode::Decode<Context> for ConversionMode {
        fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
            Ok(Self::from_bits_retain(u16::decode(decoder)?))
        }
    }
};
