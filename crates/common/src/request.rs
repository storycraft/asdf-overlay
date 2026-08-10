//! IPC request types sent from client to server.
//!
//! [`Request`] is the top-level enum representing all possible requests.

pub mod surface;
pub mod window;

use bitcode::{Decode, DecodeOwned, Encode};

use crate::{
    cursor::Cursor,
    request::{surface::SurfaceRequest, window::WindowRequest},
};

/// Describes all possible kind of requests.
#[derive(Debug, Clone, Encode, Decode, derive_more::From)]
pub enum Request {
    /// Whether to block input events from reaching all windows and listen all input events.
    BlockInput(BlockInput),

    /// Set cursor when being input blocked.
    SetBlockingCursor(SetBlockingCursor),

    /// Request to a specific window.
    Window(WindowRequest),

    /// Request to a specific surface.
    Surface(SurfaceRequest),
}

/// Trait implemented for request types.
pub trait Requestable: Into<Request> + Encode + DecodeOwned {
    type Response: Encode + DecodeOwned;
}

macro_rules! impl_Requestable {
    ($request_ty:ty, $response_ty:ty) => {
        impl $crate::request::Requestable for $request_ty {
            type Response = $response_ty;
        }
    };
}

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
/// Block input events from reaching window and listen all input events
pub struct BlockInput {
    /// Whether to block input events from reaching to window.
    pub block: bool,
}
impl_Requestable!(BlockInput, ());

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
/// Set cursor when being input blocked
pub struct SetBlockingCursor {
    /// Cursor to be set.
    /// If [`None`] is given, the cursor will be hidden.
    pub cursor: Option<Cursor>,
}
impl_Requestable!(SetBlockingCursor, ());
