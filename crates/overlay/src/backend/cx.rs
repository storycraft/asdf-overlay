use windows::Win32::Graphics::Direct3D11::ID3DDeviceContextState;

pub struct DrawContext {
    pub dx11: Option<ID3DDeviceContextState>,
}

impl DrawContext {
    pub const fn new() -> Self {
        Self { dx11: None }
    }
}

impl Default for DrawContext {
    fn default() -> Self {
        Self::new()
    }
}
