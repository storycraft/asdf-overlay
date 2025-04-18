use core::ffi::c_void;

use scopeguard::defer;
use windows::Win32::Graphics::{
    Gdi::HDC,
    OpenGL::{HGLRC, wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglMakeCurrent},
};

pub struct OverlayGlContext {
    hglrc: *mut c_void,
}

impl OverlayGlContext {
    pub fn new(hdc: HDC) -> anyhow::Result<Self> {
        let hglrc = unsafe { wglCreateContext(hdc)? }.0;

        Ok(Self { hglrc })
    }

    pub fn with<R>(&self, hdc: HDC, f: impl FnOnce() -> R) -> R {
        let original_cx = unsafe { wglGetCurrentContext() };

        unsafe { wglMakeCurrent(hdc, HGLRC(self.hglrc)).unwrap() };
        defer!(unsafe { wglMakeCurrent(hdc, original_cx).unwrap() });
        f()
    }
}

impl Drop for OverlayGlContext {
    fn drop(&mut self) {
        unsafe { wglDeleteContext(HGLRC(self.hglrc)).unwrap() };
    }
}

unsafe impl Send for OverlayGlContext {}
