//! IPC request types sent from client to server.
//!
//! [`Request`] is the top-level enum representing all possible requests.

pub mod surface;
pub mod window;

use core::fmt;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    cursor::Cursor,
    request::{surface::SurfaceRequest, window::WindowRequest},
};

/// Describes all possible kind of requests.
#[derive(Debug, Clone, derive_more::From, Serialize, Deserialize)]
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

/// Result type for IPC requests.
pub type Result<T> = core::result::Result<T, Error>;

/// Serializable error type for IPC requests.
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Error(serde_error::Error);

impl Error {
    pub fn new(error: &(impl ?Sized + core::error::Error)) -> Self {
        Self(serde_error::Error::new(error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// Trait implemented for request types.
pub trait Requestable: Into<Request> + Serialize + DeserializeOwned {
    type Response: Serialize + DeserializeOwned;
}

macro_rules! impl_Requestable {
    ($request_ty:ty, $response_ty:ty) => {
        impl $crate::request::Requestable for $request_ty {
            type Response = $response_ty;
        }
    };
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
/// Block input events from reaching window and listen all input events
pub struct BlockInput {
    /// Whether to block input events from reaching to window.
    pub block: bool,
}
impl_Requestable!(BlockInput, ());

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
/// Set cursor when being input blocked
pub struct SetBlockingCursor {
    /// Cursor to be set.
    /// If [`None`] is given, the cursor will be hidden.
    pub cursor: Option<Cursor>,
}
impl_Requestable!(SetBlockingCursor, ());
