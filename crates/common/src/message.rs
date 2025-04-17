use core::{fmt::Debug, num::NonZeroUsize};

use bincode::{Decode, Encode};

use crate::size::PercentLength;

#[derive(Debug, Encode, Decode, Clone)]
#[non_exhaustive]
pub enum Request {
    /// Change overlay position
    UpdatePosition(Position),
    /// Change overlay anchor
    UpdateAnchor(Anchor),
    /// Change overlay anchor
    UpdateMargin(Margin),

    /// Update overlay to new texture using shared dx11 texture handle
    UpdateShtex(SharedHandle),
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum Response {
    Success,
    Failed { message: String },
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

#[derive(Debug, Encode, Decode, Clone)]
pub struct SharedHandle {
    pub handle: Option<NonZeroUsize>,
}
