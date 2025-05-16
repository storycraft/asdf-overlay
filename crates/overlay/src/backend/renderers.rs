use crate::renderer::{dx9::Dx9Renderer, dx11::Dx11Renderer, dx12::Dx12Renderer};

pub struct Renderer {
    pub dx12: Option<Dx12Renderer>,
    pub dx11: Option<Dx11Renderer>,
    pub dx9: Option<Dx9Renderer>,
}

impl Renderer {
    pub const fn new() -> Self {
        Self {
            dx12: None,
            dx11: None,
            dx9: None,
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
