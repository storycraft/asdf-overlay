use core::mem::ManuallyDrop;

use anyhow::Ok;
use scopeguard::defer;
use windows::Win32::Graphics::{
    Gdi::HDC,
    OpenGL::{
        HGLRC, wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglGetCurrentDC,
        wglMakeCurrent,
    },
};

pub struct WglContextWrapped<T: ?Sized> {
    cx: WglContext,
    inner: ManuallyDrop<T>,
}

impl<T: ?Sized> WglContextWrapped<T> {
    pub fn new_with(hdc: HDC, f: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<Self>
    where
        T: Sized,
    {
        let mut cx = WglContext::new(hdc)?;
        let inner = ManuallyDrop::new(cx.with(f)?);
        Ok(Self { cx, inner })
    }

    #[inline]
    pub fn with<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        self.cx.with(|| f(&mut self.inner))
    }
}

impl<T: ?Sized> Drop for WglContextWrapped<T> {
    fn drop(&mut self) {
        self.cx.with(|| unsafe {
            ManuallyDrop::drop(&mut self.inner);
        });
    }
}

struct WglContext {
    hdc: HDC,
    hglrc: HGLRC,
}

impl WglContext {
    pub fn new(hdc: HDC) -> anyhow::Result<Self> {
        let hglrc = unsafe { wglCreateContext(hdc)? };

        Ok(Self { hdc, hglrc })
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
