use core::{fmt::Debug, num::NonZeroUsize};

use bincode::{Decode, Encode};

use crate::size::PercentLength;

#[derive(Debug, Encode, Decode, Clone)]
pub enum Request {
    SetPosition(SetPosition),
    SetAnchor(SetAnchor),
    SetMargin(SetMargin),
    GetSize(GetSize),
    SetInputCapture(SetInputCapture),
    UpdateSharedHandle(UpdateSharedHandle),
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay position
pub struct SetPosition {
    pub x: PercentLength,
    pub y: PercentLength,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay anchor
pub struct SetAnchor {
    pub x: PercentLength,
    pub y: PercentLength,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Change overlay margin
pub struct SetMargin {
    pub top: PercentLength,
    pub right: PercentLength,
    pub bottom: PercentLength,
    pub left: PercentLength,
}

impl SetMargin {
    pub const fn xy(x: PercentLength, y: PercentLength) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Get size of overlay window
pub struct GetSize {
    pub hwnd: u32,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set input capture of a overlay window
pub struct SetInputCapture {
    pub hwnd: u32,
    pub capture: bool,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq)]
/// Update overlay to new texture using shared dx11 texture handle
pub struct UpdateSharedHandle {
    pub handle: Option<NonZeroUsize>,
}
