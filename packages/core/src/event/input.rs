use asdf_overlay_client::common::event::window::input;
use napi_derive::napi;

use crate::event::ime::Ime;

pub enum InputEvent {
    Cursor { event: CursorInput },
    Keyboard { event: KeyboardInput },
}

impl From<input::InputEvent> for InputEvent {
    fn from(event: input::InputEvent) -> Self {
        match event {
            input::InputEvent::Cursor(cursor) => InputEvent::Cursor {
                event: cursor.into(),
            },
            input::InputEvent::Keyboard(keyboard) => InputEvent::Keyboard {
                event: keyboard.into(),
            },
        }
    }
}

/// Describe a cursor input.
#[napi(object)]
pub struct CursorInput {
    /// X position relative to window.
    pub x: i32,

    /// Y position relative to window.
    pub y: i32,

    pub kind: CursorInputKind,
}

impl From<input::CursorInput> for CursorInput {
    fn from(input: input::CursorInput) -> Self {
        CursorInput {
            x: input.pos.x,
            y: input.pos.y,
            kind: input.event.into(),
        }
    }
}

#[napi]
pub enum CursorInputKind {
    /// Cursor has entered to a windowv
    Enter,

    /// Cursor has left from a window
    Leave,

    /// Cursor has moved
    Move,

    /// Cursor button has been pressed or released
    Action {
        action: CursorAction,
        state: CursorInputState,
    },

    /// Cursor wheel has scrolled
    Scroll { axis: ScrollAxis, delta: i16 },
}

impl From<input::CursorEvent> for CursorInputKind {
    fn from(event: input::CursorEvent) -> Self {
        match event {
            input::CursorEvent::Enter => CursorInputKind::Enter,
            input::CursorEvent::Leave => CursorInputKind::Leave,
            input::CursorEvent::Move => CursorInputKind::Move,
            input::CursorEvent::Action { state, action } => CursorInputKind::Action {
                action: action.into(),
                state: state.into(),
            },
            input::CursorEvent::Scroll { axis, delta } => CursorInputKind::Scroll {
                axis: axis.into(),
                delta,
            },
        }
    }
}

#[napi]
pub enum KeyboardInput {
    /// A key is pressed or released.
    Key { key: Key, state: KeyInputState },

    /// A character input due to a key press without involving IME.
    Char {
        /// Input character (1 character).
        ch: String,
    },

    /// IME related event.
    Ime {
        /// An IME event.
        ime: Ime,
    },
}

impl From<input::KeyboardInput> for KeyboardInput {
    fn from(input: input::KeyboardInput) -> Self {
        match input {
            input::KeyboardInput::Key { key, state } => KeyboardInput::Key {
                key: key.into(),
                state: state.into(),
            },
            input::KeyboardInput::Char(ch) => KeyboardInput::Char { ch: ch.to_string() },
            input::KeyboardInput::Ime(ime) => KeyboardInput::Ime { ime: ime.into() },
        }
    }
}

/// Describe a virtual key code.
#[napi(object)]
pub struct Key {
    /// A Windows Virtual-Key code.
    pub code: u8,

    /// Whether if this key is an extended key.
    ///
    /// This is usually true for right-side modifier keys, numpad keys, and arrow keys.
    pub extended: bool,
}

impl From<input::Key> for Key {
    fn from(key: input::Key) -> Self {
        Key {
            code: key.code.get(),
            extended: key.extended,
        }
    }
}

/// Utility function to create `Key` using key code and optional extended flag.
#[napi]
pub fn key(code: u8, extended: Option<bool>) -> Key {
    Key {
        code,
        extended: extended.unwrap_or(false),
    }
}

/// Cursor scroll axis.
#[napi(string_enum)]
pub enum ScrollAxis {
    X,
    Y,
}

impl From<input::ScrollAxis> for ScrollAxis {
    fn from(axis: input::ScrollAxis) -> Self {
        match axis {
            input::ScrollAxis::X => ScrollAxis::X,
            input::ScrollAxis::Y => ScrollAxis::Y,
        }
    }
}

/// Cursor buttons.
#[napi(string_enum)]
pub enum CursorAction {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl From<input::CursorAction> for CursorAction {
    fn from(action: input::CursorAction) -> Self {
        match action {
            input::CursorAction::Left => CursorAction::Left,
            input::CursorAction::Right => CursorAction::Right,
            input::CursorAction::Middle => CursorAction::Middle,
            input::CursorAction::Back => CursorAction::Back,
            input::CursorAction::Forward => CursorAction::Forward,
        }
    }
}

/// Cursor input state.
#[napi]
pub enum CursorInputState {
    Pressed { double_click: bool },
    Released,
}

impl From<input::CursorInputState> for CursorInputState {
    fn from(state: input::CursorInputState) -> Self {
        match state {
            input::CursorInputState::Pressed { double_click } => {
                CursorInputState::Pressed { double_click }
            }
            input::CursorInputState::Released => CursorInputState::Released,
        }
    }
}

/// Key input state.
#[napi(string_enum)]
pub enum KeyInputState {
    /// The key is pressed down.
    Pressed,

    /// The key is released.
    Released,
}

impl From<input::KeyInputState> for KeyInputState {
    fn from(state: input::KeyInputState) -> Self {
        match state {
            input::KeyInputState::Pressed => KeyInputState::Pressed,
            input::KeyInputState::Released => KeyInputState::Released,
        }
    }
}

#[napi]
pub enum Cursor {
    Default = 0,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    EastWestResize,
    NorthSouthResize,
    NorthEastSouthWestResize,
    NorthWestSouthEastResize,
    ZoomIn,
    ZoomOut,

    // Windows additional cursors
    UpArrow,
    Pin,
    Person,
    Pen,
    Cd,

    // Panning cursors
    PanMiddle,
    PanMiddleHorizontal,
    PanMiddleVertical,
    PanEast,
    PanNorth,
    PanNorthEast,
    PanNorthWest,
    PanSouth,
    PanSouthEast,
    PanSouthWest,
    PanWest,
}

#[napi]
pub enum ImeConversion {
    None = 0,

    /// IME converts to native langauge.
    Native = 1,

    /// IME composes in full-width characters.
    Fullshape = 2,

    /// Conversion is disabled.
    NoConversion = 4,

    /// Converting to hanja.
    HanjaConvert = 8,

    /// Converting to katakana.
    Katakana = 16,
}
