//! IPC request types sent from client to server.
//!
//! [`Request`] is the top-level enum representing all possible requests.

use core::{fmt::Debug, num::NonZeroU32};

use bincode::{Decode, Encode};

use crate::{cursor::Cursor, size::PercentLength};

/// Describes a request.
#[derive(Debug, Encode, Decode, Clone)]
pub enum Request {
    /// Request to a specific window.
    Window {
        /// Window identifier.
        id: u32,

        /// The underlying window request.
        request: WindowRequest,
    },
}

/// Describes a request to a specific window.
#[derive(Debug, Encode, Decode, Clone, derive_more::From)]
pub enum WindowRequest {
    /// Set overlay surface position.
    SetPosition(SetPosition),

    /// Set overlay surface positioning anchor.
    SetAnchor(SetAnchor),

    /// Set overlay surface margin.
    SetMargin(SetMargin),

    /// Change whether to listen input events.
    ListenInput(ListenInput),

    /// Whether to block input events from reaching window and listen all input events.
    BlockInput(BlockInput),

    /// Set cursor of a window when being input blocked.
    SetBlockingCursor(SetBlockingCursor),

    /// Set overlay surface shared handle.
    UpdateSharedHandle(UpdateSharedHandle),
}

mod __sealed {
    pub trait Sealed {}
}

/// Trait implemented to sub types of [`WindowRequest`] enum.
pub trait WindowRequestItem: __sealed::Sealed + Into<WindowRequest> {}

macro_rules! impl_WindowRequestItem {
    ($ty:ty) => {
        impl __sealed::Sealed for $ty {}
        impl WindowRequestItem for $ty {}
    };
}

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay surface position relative to the window client area.
pub struct SetPosition {
    /// X position of percent or absolute length relative to the window's width.
    pub x: PercentLength,

    /// Y position of percent or absolute length relative to the window's height.
    pub y: PercentLength,
}
impl_WindowRequestItem!(SetPosition);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay surface positioning anchor relative to the top-left of the overlay surface.
pub struct SetAnchor {
    /// Anchor of X axis as percent relative to the window's width.
    pub x: PercentLength,

    /// Anchor of Y axis as percent relative to the window's height.
    pub y: PercentLength,
}
impl_WindowRequestItem!(SetAnchor);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq)]
/// Set overlay surface margin relative to the overlay surface's size.
pub struct SetMargin {
    /// Margin of top side as percent or absolute length relative to the overlay surface's height.
    pub top: PercentLength,

    /// Margin of right side as percent or absolute length relative to the overlay surface's width.
    pub right: PercentLength,

    /// Margin of bottom side as percent or absolute length relative to the overlay surface's height.
    pub bottom: PercentLength,

    /// Margin of left side as percent or absolute length relative to the overlay surface's width.
    pub left: PercentLength,
}
impl_WindowRequestItem!(SetMargin);

impl SetMargin {
    /// Utility function to set same margin for axis.
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
/// Listen input events.
pub struct ListenInput {
    /// Whether to listen cursor related events.
    pub cursor: bool,

    /// Whether to listen keyboard related events.
    pub keyboard: bool,
}
impl_WindowRequestItem!(ListenInput);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Block input events from reaching window and listen all input events
pub struct BlockInput {
    /// Whether to block input events from reaching to window.
    pub block: bool,
}
impl_WindowRequestItem!(BlockInput);

#[derive(Debug, Default, Encode, Decode, Clone, PartialEq, Eq)]
/// Set cursor of a window being input captured
pub struct SetBlockingCursor {
    /// Cursor to be set.
    /// If [`None`] is given, the cursor will be hidden.
    pub cursor: Option<Cursor>,
}
impl_WindowRequestItem!(SetBlockingCursor);

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq)]
/// Update overlay surface
pub struct UpdateSharedHandle {
    /// DirectX KMT shared handle to the overlay surface texture.
    ///
    /// ## Note
    /// * The texture must be a 32-bit BGRA format texture.
    /// * The texture must be created with `D3D11_RESOURCE_MISC_SHARED` flag.
    /// * If the texture is created with `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX` flag, the `key` of the `IDXGIKeyedMutex` must be `0`.
    ///
    /// If [`None`] is given, the overlay surface will be removed.
    pub handle: Option<NonZeroU32>,
}
impl_WindowRequestItem!(UpdateSharedHandle);
