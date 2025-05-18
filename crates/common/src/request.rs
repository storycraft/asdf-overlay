use core::{fmt::Debug, num::NonZeroUsize};

use bincode::{Decode, Encode};

use crate::{cursor::Cursor, size::PercentLength};

#[derive(Debug, Encode, Decode, Clone)]
pub enum Request {
    SetPosition(SetPosition),
    SetAnchor(SetAnchor),
    SetMargin(SetMargin),
    GetSize(GetSize),
    ListenInputEvent(ListenInputEvent),
    BlockInput(BlockInput),
    SetBlockingCursor(SetBlockingCursor),
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

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Get size of overlay window
pub struct GetSize {
    pub hwnd: u32,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Listen input events of a window
pub struct ListenInputEvent {
    pub hwnd: u32,
    pub cursor: bool,
    pub keyboard: bool,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Block inputs from reaching window
pub struct BlockInput {
    pub hwnd: u32,
    pub block: bool,
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Set cursor while input is blocked
pub struct SetBlockingCursor {
    pub hwnd: u32,
    pub cursor: Option<Cursor>,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
/// Update overlay to new texture using shared dx11 texture handle
pub struct UpdateSharedHandle {
    pub handle: Option<NonZeroUsize>,
}
