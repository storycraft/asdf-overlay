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

/// Update overlay surface
///
/// ## Note
/// * If the texture is created with `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX` flag, the `key` of the `IDXGIKeyedMutex` must be `0`.
///
/// [`UpdateSharedHandle::None`] removes the overlay texture, not the tracked
/// graphics surface. The response is `()`. Unknown surface IDs and texture-open
/// failures are errors; acknowledgment does not wait for presentation. The texture
/// must be compatible with the target surface's GPU adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateSharedHandle {
    /// A legacy KMT shared handle; keep the source resource alive while it is used.
    ///
    /// An NT handle is not a valid substitute, even though both are stored as `u32`.
    Kmt(u32),

    /// An NT shared handle already valid in the server process.
    ///
    /// The protocol does not duplicate a client-local handle. On a successful
    /// update the server owns and eventually closes it; opening failure does not
    /// close it. Copying this enum does not duplicate the underlying handle.
    Nt(u32),

    /// Remove the overlay surface.
    None,
}

impl_SurfaceRequestable!(UpdateSharedHandle, ());
