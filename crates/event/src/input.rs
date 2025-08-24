//! Input event types and related types for IPC.
//!
//! Raw inputs are not handled here, as they can be done from the client side directly.

use core::num::NonZeroU8;

/// Describe an input event captured from a window.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum InputEvent {
    /// A cursor input.
    Cursor(CursorInput),
    /// A keyboard input.
    Keyboard(KeyboardInput),
}

/// Describe a cursor related input.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct CursorInput {
    /// The type of cursor input.
    pub event: CursorEvent,
    /// Cursor position relative to overlay surface position.
    pub client: InputPosition,
    /// Cursor position relative to the left-top corner client area of the window.
    pub window: InputPosition,
}

/// Describe a cursor event.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum CursorEvent {
    /// Cursor entered the window from outside of the window.
    Enter,

    /// Cursor left the window to outside of the window.
    ///
    /// Note that this event is also sent when input blocking is ended.
    Leave,

    /// A cursor button is pressed or released.
    Action {
        /// The state of the input.
        state: CursorInputState,

        /// The button for this action.
        action: CursorAction,
    },

    /// Cursor is moved.
    Move,

    /// Wheel is scrolled.
    Scroll {
        /// The axis of the scroll.
        axis: ScrollAxis,

        /// The scroll delta. Positive value means scrolling down/right, negative value means scrolling up/left.
        ///
        /// The actual scroll amount may vary depending on the platform and user settings.
        delta: i16,
    },
}

/// Describe the state of a cursor button input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum CursorInputState {
    /// Button is pressed down.
    Pressed {
        /// Whether if this click should be treated as part of last click of double clicking.
        ///
        /// The actual timing is platform and user setting dependent.
        double_click: bool,
    },

    /// Button is released.
    Released,
}

/// Describe a keyboard related input.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum KeyboardInput {
    /// An raw Key input which does not consider keyboard layout.
    ///
    /// The key code does not diminish left/right variant of modifier keys.
    /// To distinguish left/right modifier keys, check the [`Key::extended`] field.
    Key {
        /// The key code of the input.
        key: Key,

        /// The state of the key input.
        state: KeyInputState,
    },

    /// A character input without involving IME.
    ///
    /// This is usually sent after [`KeyboardInput::Key`] with [`KeyInputState::Pressed`] state
    /// if there was a printable character associated with the key.
    ///
    /// The character may be different from the key code due to keyboard layout or modifier keys.
    Char(char),

    /// IME involved input.
    Ime(Ime),
}

/// Describe a virtual key code.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct Key {
    /// A Windows Virtual-Key code.
    ///
    /// Refer to [Virtual-Key Codes](https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes) for details.
    pub code: NonZeroU8,

    /// Whether if this key is an extended key.
    ///
    /// This is usually true for right-side modifier keys, numpad keys, and arrow keys.
    pub extended: bool,
}

impl Key {
    /// Create a new [`Key`] from a virtual-key code.
    pub fn new(code: u8, extended: bool) -> Option<Self> {
        NonZeroU8::new(code).map(|code| Key { code, extended })
    }
}

/// Describe a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum CursorAction {
    /// Left button
    Left,

    /// Right button
    Right,

    /// Wheel button
    Middle,

    /// Extra button 1 (usually mapped to `Back` action)
    Back,

    /// Extra button 2 (usually mapped to `Forward` action)
    Forward,
}

/// Describe a scroll axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum ScrollAxis {
    /// Horizontal axis
    X,

    /// Vertical axis
    Y,
}

/// Describe the state of a key input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum KeyInputState {
    /// The key is pressed down.
    Pressed,

    /// The key is released.
    Released,
}

/// Describe a 2D position for cursor input.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct InputPosition {
    /// X position in pixels.
    ///
    /// Positive value means right, negative value means left from the origin.
    pub x: i32,

    /// Y position in pixels.
    ///
    /// Positive value means down, negative value means up from the origin.
    pub y: i32,
}

/// Describe an IME input.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum Ime {
    /// IME is enabled due to various reasons, such as window gained focus.
    Enabled {
        /// The initial language of the IME.
        ///
        /// The language is in [BCP 47](https://tools.ietf.org/html/bcp47) format.
        lang: String,

        /// The initial conversion mode of the IME
        conversion: ConversionMode,
    },

    /// IME language is changed.
    ///
    /// The language is in [BCP 47](https://tools.ietf.org/html/bcp47) format.
    Changed(String),

    /// IME conversion mode changed
    ConversionChanged(ConversionMode),

    /// IME candidates window is opened or changed
    CandidateChanged(ImeCandidateList),

    /// IME candidates window is closed
    CandidateClosed,

    /// IME is composing text
    Compose {
        /// The composing text
        text: String,

        /// The start index of the selection range in the composing text.
        caret: usize,
    },

    /// IME commit the composing text.
    ///
    /// You must clear any existing composing text when you receive this event.
    Commit(String),

    /// IME is disabled due to various reasons, such as window lost focus.
    Disabled,
}

/// Describe a list of IME candidates.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct ImeCandidateList {
    /// The start index of the current page in the candidates list.
    pub page_start_index: u32,

    /// The number of candidates per page.
    pub page_size: u32,

    /// The selected index in the candidates list.
    pub selected_index: u32,

    /// The list of candidate strings.
    pub candidates: Vec<String>,
}

bitflags::bitflags! {
    /// Describe IME conversion modes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConversionMode: u16 {
        /// Composing in native language of the IME.
        const NATIVE = 1;

        /// Composing in full-width characters.
        const FULLSHAPE = 1 << 1;

        /// Conversion is disabled.
        const NO_CONVERSION = 1 << 2;

        /// Hanja conversion mode.
        const HANJA_CONVERT = 1 << 3;

        /// Katakana conversion mode.
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
