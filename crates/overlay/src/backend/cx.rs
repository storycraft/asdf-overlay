use core::ffi::c_void;

use scopeguard::defer;
use windows::Win32::Graphics::{
    Direct3D11::ID3DDeviceContextState,
    Gdi::HDC,
    OpenGL::{HGLRC, wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglMakeCurrent},
};

pub struct DrawContext {
    pub opengl: Option<GlContext>,
    pub dx11: Option<ID3DDeviceContextState>,
}

impl DrawContext {
    pub const fn new() -> Self {
        Self {
            opengl: None,
            dx11: None,
        }
    }
}

impl Default for DrawContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GlContext {
    hglrc: *mut c_void,
}

impl GlContext {
    pub fn new(hdc: HDC) -> anyhow::Result<Self> {
        let hglrc = unsafe { wglCreateContext(hdc)? }.0;

        Ok(Self { hglrc })
    }

    pub fn with<R>(&mut self, hdc: HDC, f: impl FnOnce() -> R) -> R {
        let original_cx = unsafe { wglGetCurrentContext() };

        unsafe { wglMakeCurrent(hdc, HGLRC(self.hglrc)).unwrap() };
        defer!(unsafe { wglMakeCurrent(hdc, original_cx).unwrap() });
        f()
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe { _ = wglDeleteContext(HGLRC(self.hglrc)) };
    }
}

unsafe impl Send for GlContext {}
unsafe impl Sync for GlContext {}
