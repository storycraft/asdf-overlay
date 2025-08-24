//! [`Cursor`] enum representing various cursor types.

use bincode::{Decode, Encode};
use num_derive::FromPrimitive;

/// Describes a possible cursor type.
#[derive(Debug, Default, Encode, Decode, FromPrimitive, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Cursor {
    /// The platform default cursor. Typically an arrow.
    #[default]
    Default = 0,

    /// A cursor indicating that helpful information is available.
    Help,

    /// A cursor indicating that something is clickable, typically a pointing hand.
    Pointer,

    /// The program is busy at background but can still interactable. Typically an arrow with a loading indicator.
    Progress,

    /// The program is busy and cannot interact. Typically an hourglass or spinning circle.
    Wait,

    /// Table cell selection cursor
    Cell,

    /// Cross cursor typically used for precision selection, e.g. in graphics applications.
    Crosshair,

    /// A horizontal ibeam cursor for indicating text insertion, typically used for text.
    Text,

    /// A vertical ibeam cursor for indicating text insertion, typically used for vertical text.
    VerticalText,

    /// Cursor for indicating something can be aliased or shortcuted.
    Alias,

    /// Cursor for indicating something can be copied.
    Copy,

    /// Crossed arrows for moving something.
    Move,

    /// Not allowed cursor (typically a circle with a line through it)
    NotAllowed,

    /// Hand cursor for something can be grabbed. e.g. draggable item
    Grab,

    /// Hand grabbing cursor.
    Grabbing,

    /// Cursor for horizontal resizing (e.g. left-right arrows with a bar in the middle)
    ColResize,

    /// Cursor for vertical resizing (e.g. up-down arrows with a bar in the middle)
    RowResize,

    /// Cursor for horizontal resizing (e.g. left-right arrows)
    EastWestResize,

    /// Cursor for vertical resizing (e.g. up-down arrows)
    NorthSouthResize,

    /// Cursor for diagonal resizing (e.g. top-left to bottom-right arrows)
    NorthEastSouthWestResize,

    /// Cursor for diagonal resizing (e.g. top-right to bottom-left arrows)
    NorthWestSouthEastResize,

    /// Cursor for something can be zoomed in. Typically a magnifying glass with a plus sign.
    ZoomIn,

    /// Cursor for something can be zoomed out. Typically a magnifying glass with a minus sign.
    ZoomOut,

    /// Windows specific up arrow cursor matching `IDC_UPARROW`
    UpArrow,

    /// Windows specific pin cursor matching `IDC_PIN`
    Pin,

    /// Windows specific person cursor matching `IDC_PERSON`
    Person,

    /// Windows specific pen cursor matching `32631`
    Pen,

    /// Windows specific cd cursor matching `32663`
    Cd,

    /// Panning cursors with crossed arrows
    PanMiddle,

    /// Panning cursors with horizontal arrows
    PanMiddleHorizontal,

    /// Panning cursors with vertical arrows
    PanMiddleVertical,

    /// Panning cursors with a arrow to right.
    PanEast,

    /// Panning cursors with a arrow to top.
    PanNorth,

    /// Panning cursors with arrows to top and right.
    PanNorthEast,

    /// Panning cursors with arrows to top and left.
    PanNorthWest,

    /// Panning cursors with a arrow to down.
    PanSouth,

    /// Panning cursors with arrows to down and right.
    PanSouthEast,

    /// Panning cursors with arrows to down and left.
    PanSouthWest,

    /// Panning cursors with a arrow to left.
    PanWest,
}
