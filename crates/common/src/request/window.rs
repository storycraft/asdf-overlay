//! Window request types.

use core::fmt::Debug;

use bitcode::{Decode, DecodeOwned, Encode};

/// Describes all possible kinds of window request.
#[derive(Debug, Clone, Encode, Decode)]
pub struct WindowRequest {
    /// Window identifier.
    pub id: u32,

    /// The underlying window request.
    pub kind: WindowRequestKind,
}

#[derive(Debug, Clone, derive_more::From, Encode, Decode)]
pub enum WindowRequestKind {
    /// Change whether to listen input events.
    ListenInput(ListenInput),
}

/// Trait implemented to sub types of [`WindowRequest`] enum.
pub trait WindowRequestable: Into<WindowRequestKind> + Encode + DecodeOwned {
    type Response: Encode + DecodeOwned;
}

macro_rules! impl_WindowRequestable {
    ($ty:ty, $res_ty:ty) => {
        impl WindowRequestable for $ty {
            type Response = $res_ty;
        }
    };
}

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
/// Listen input events.
pub struct ListenInput {
    /// Whether to listen cursor related events.
    pub cursor: bool,

    /// Whether to listen keyboard related events.
    pub keyboard: bool,
}
impl_WindowRequestable!(ListenInput, ());
