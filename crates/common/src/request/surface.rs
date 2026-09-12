use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Describes all possible kinds of surface request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceRequest {
    /// Surface identifier.
    pub id: u64,

    /// The underlying surface request.
    pub kind: SurfaceRequestKind,
}

#[derive(Debug, Clone, derive_more::From, Serialize, Deserialize)]
pub enum SurfaceRequestKind {
    /// Set overlay surface position.
    SetPosition(SetPosition),

    /// Set overlay shared handle.
    UpdateSharedHandle(UpdateSharedHandle),
}

/// Trait implemented to sub types of [`SurfaceRequestKind`] enum.
pub trait SurfaceRequestable: Into<SurfaceRequestKind> + Serialize + DeserializeOwned {
    type Response: Serialize + DeserializeOwned;
}

macro_rules! impl_SurfaceRequestable {
    ($ty:ty, $res_ty:ty) => {
        impl SurfaceRequestable for $ty {
            type Response = $res_ty;
        }
    };
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
/// Set overlay surface position relative to the window client area.
pub struct SetPosition {
    /// X position.
    pub x: i32,

    /// Y position.
    pub y: i32,
}
impl_SurfaceRequestable!(SetPosition, ());

/// Replace or remove an overlay texture.
///
/// The texture must use the target surface's GPU adapter and release any keyed
/// mutex at key zero for rendering. Unknown surfaces and texture-opening failures
/// return errors. Acknowledgment does not wait for presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateSharedHandle {
    /// A legacy KMT shared handle. Keep the source resource alive while in use.
    Kmt(u32),

    /// An NT shared handle valid in the server process.
    ///
    /// The server takes ownership on success; the caller retains ownership on failure.
    /// Copying this value does not duplicate the handle.
    Nt(u32),

    /// Remove the overlay texture while retaining the tracked surface.
    None,
}

impl_SurfaceRequestable!(UpdateSharedHandle, ());
