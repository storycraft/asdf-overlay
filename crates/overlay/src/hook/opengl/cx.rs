use anyhow::Ok;
use scopeguard::defer;
use windows::Win32::Graphics::{
    Gdi::HDC,
    OpenGL::{
        HGLRC, wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglGetCurrentDC,
        wglMakeCurrent,
    },
};

pub struct WglContext {
    hdc: HDC,
    hglrc: HGLRC,
}

impl WglContext {
    pub fn new(hdc: HDC) -> anyhow::Result<Self> {
        let hglrc = unsafe { wglCreateContext(hdc)? };

        Ok(Self { hdc, hglrc })
    }

    pub fn hglrc(&mut self) -> HGLRC {
        self.hglrc
    }

    pub fn with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let last_hdc = unsafe { wglGetCurrentDC() };
        let original_cx = unsafe { wglGetCurrentContext() };

        unsafe { wglMakeCurrent(self.hdc, self.hglrc).unwrap() };
        defer!(unsafe { wglMakeCurrent(last_hdc, original_cx).unwrap() });
        f()
    }
}

impl Drop for WglContext {
    fn drop(&mut self) {
        unsafe { _ = wglDeleteContext(self.hglrc) };
    }
}

unsafe impl Send for WglContext {}
unsafe impl Sync for WglContext {}
