use bincode::{Decode, Encode};

use crate::size::PercentLength;

#[derive(Debug, Encode, Decode)]
pub enum Request {
    /// Change overlay position
    UpdatePosition(Position),
    /// Change overlay anchor
    UpdateAnchor(Anchor),
    /// Change overlay anchor
    UpdateMargin(Margin),

    /// Update overlay using bitmap
    UpdateBitmap(Bitmap),
    /// Update overlay using shared dx11 texture handle
    Direct(UpdateDirect),
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
pub struct Position {
    pub x: PercentLength,
    pub y: PercentLength,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
pub struct Anchor {
    pub x: PercentLength,
    pub y: PercentLength,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
pub struct Margin {
    pub top: PercentLength,
    pub right: PercentLength,
    pub bottom: PercentLength,
    pub left: PercentLength,
}

impl Margin {
    pub const fn xy(x: PercentLength, y: PercentLength) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }
}

#[derive(Debug, Encode, Decode)]
pub struct Bitmap {
    pub width: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Encode, Decode)]
pub struct UpdateDirect {
    pub width: u32,
    pub handle: usize,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum Response {
    Success,
    Failed { message: String },
}
