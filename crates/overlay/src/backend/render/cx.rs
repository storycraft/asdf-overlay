pub mod dx12;

use windows::Win32::Graphics::Direct3D11::ID3DDeviceContextState;

use crate::backend::render::cx::dx12::RtvDescriptors;

pub struct DrawContext {
    pub dx11: Option<ID3DDeviceContextState>,
    pub dx12: Option<RtvDescriptors>,
}

impl DrawContext {
    pub const fn new() -> Self {
        Self {
            dx11: None,
            dx12: None,
        }
    }
}

impl Default for DrawContext {
    fn default() -> Self {
        Self::new()
    }
}
