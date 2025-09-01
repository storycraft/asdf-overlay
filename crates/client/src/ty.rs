//! Various types used in the [`crate::surface`] module.

/// Describes a rectangle area.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    /// X coordinate of the upper-left corner of the rectangle.
    pub x: u32,

    /// Y coordinate of the upper-left corner of the rectangle.
    pub y: u32,

    /// Width of the rectangle.
    pub width: u32,

    /// Height of the rectangle.
    pub height: u32,
}

/// Describes a rectangle area to be copied from a source texture to a destination texture.
#[derive(Debug, Clone, Copy)]
pub struct CopyRect {
    /// X coordinate of the upper-left corner of the destination rectangle.
    pub dst_x: u32,

    /// Y coordinate of the upper-left corner of the destination rectangle.
    pub dst_y: u32,

    /// Source rectangle area.
    pub src: Rect,
}
