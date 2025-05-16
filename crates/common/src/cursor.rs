use bincode::{Decode, Encode};
use num_derive::FromPrimitive;

#[derive(Debug, Default, Encode, Decode, FromPrimitive, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Cursor {
    // web cursors
    #[default]
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

    // panning
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
