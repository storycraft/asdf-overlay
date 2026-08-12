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
/// If [`UpdateSharedHandle::None`] is given, the overlay surface will be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateSharedHandle {
    /// A KMT shared handle.
    Kmt(u32),

    /// A NT shared handle.
    Nt(u32),

    /// Remove the overlay surface.
    None,
}

impl_SurfaceRequestable!(UpdateSharedHandle, ());
