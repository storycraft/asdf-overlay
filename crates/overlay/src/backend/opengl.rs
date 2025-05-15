use core::mem::ManuallyDrop;

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
    pub fn new_with(mut cx: WglContext, f: impl FnOnce() -> T) -> Self
    where
        T: Sized,
    {
        let inner = ManuallyDrop::new(cx.with(f));
        Self { cx, inner }
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

pub struct WglContext {
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
        let hdc = self.hdc;

        unsafe { wglMakeCurrent(last_hdc, self.hglrc).unwrap() };
        defer!(unsafe { wglMakeCurrent(hdc, original_cx).unwrap() });
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
