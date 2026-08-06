use asdf_overlay_event::input;
use napi_derive::napi;

/// Cursor scroll axis.
#[napi(string_enum)]
pub enum ScrollAxis {
    X,
    Y,
}

impl From<ScrollAxis> for input::ScrollAxis {
    fn from(axis: ScrollAxis) -> Self {
        match axis {
            ScrollAxis::X => input::ScrollAxis::X,
            ScrollAxis::Y => input::ScrollAxis::Y,
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

impl From<CursorAction> for input::CursorAction {
    fn from(action: CursorAction) -> Self {
        match action {
            CursorAction::Left => input::CursorAction::Left,
            CursorAction::Right => input::CursorAction::Right,
            CursorAction::Middle => input::CursorAction::Middle,
            CursorAction::Back => input::CursorAction::Back,
            CursorAction::Forward => input::CursorAction::Forward,
        }
    }
}

/// Key input state.
#[napi(string_enum)]
pub enum InputState {
    Pressed,
    Released,
}

impl From<InputState> for input::KeyInputState {
    fn from(state: InputState) -> Self {
        match state {
            InputState::Pressed => input::KeyInputState::Pressed,
            InputState::Released => input::KeyInputState::Released,
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
