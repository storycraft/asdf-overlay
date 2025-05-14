use windows::Win32::Graphics::Direct3D11::ID3DDeviceContextState;

use crate::reader::SharedHandleReader;

pub struct DrawContext {
    pub dx11: Option<ID3DDeviceContextState>,
    pub fallback_reader: Option<SharedHandleReader>,
}

impl DrawContext {
    pub const fn new() -> Self {
        Self {
            dx11: None,
            fallback_reader: None,
        }
    }
}

impl Default for DrawContext {
    fn default() -> Self {
        Self::new()
    }
}
