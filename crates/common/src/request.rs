use core::{fmt::Debug, num::NonZeroU32};

use bincode::{Decode, Encode};

use crate::{cursor::Cursor, size::PercentLength};

#[derive(Debug, Encode, Decode, Clone)]
pub enum Request {
    Window { id: u32, request: WindowRequest },
}

#[derive(Debug, Encode, Decode, Clone, derive_more::From)]
pub enum WindowRequest {
    SetPosition(SetPosition),
    SetAnchor(SetAnchor),
    SetMargin(SetMargin),
    ListenInput(ListenInput),
    BlockInput(BlockInput),
    SetBlockingCursor(SetBlockingCursor),
    UpdateSharedHandle(UpdateSharedHandle),
}

mod __sealed {
    pub trait Sealed {}
}

/// Requests in [`WindowRequest`]
pub trait WindowRequestItem: __sealed::Sealed + Into<WindowRequest> {}

macro_rules! impl_WindowRequestItem {
    ($ty:ty) => {
        impl __sealed::Sealed for $ty {}
        impl WindowRequestItem for $ty {}
    };
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay position
pub struct SetPosition {
    pub x: PercentLength,
    pub y: PercentLength,
}
impl_WindowRequestItem!(SetPosition);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay positioning anchor
pub struct SetAnchor {
    pub x: PercentLength,
    pub y: PercentLength,
}
impl_WindowRequestItem!(SetAnchor);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay margin
pub struct SetMargin {
    pub top: PercentLength,
    pub right: PercentLength,
    pub bottom: PercentLength,
    pub left: PercentLength,
}
impl_WindowRequestItem!(SetMargin);

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
/// Listen input events
pub struct ListenInput {
    pub cursor: bool,
    pub keyboard: bool,
}
impl_WindowRequestItem!(ListenInput);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Block input events from reaching window and listen all input events
pub struct BlockInput {
    pub block: bool,
}
impl_WindowRequestItem!(BlockInput);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Set cursor of a window being input captured
pub struct SetBlockingCursor {
    pub cursor: Option<Cursor>,
}
impl_WindowRequestItem!(SetBlockingCursor);

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
/// Update overlay surface
pub struct UpdateSharedHandle {
    pub handle: Option<NonZeroU32>,
}
impl_WindowRequestItem!(UpdateSharedHandle);
