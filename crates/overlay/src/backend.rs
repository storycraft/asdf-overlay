use core::{ffi::c_void, mem, ptr};

use anyhow::bail;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use rustc_hash::FxBuildHasher;
use tracing::{debug, trace};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWLP_WNDPROC, SetWindowLongPtrW, WNDPROC,
    },
};

use crate::renderer::{
    dx9::Dx9Renderer, dx11::Dx11Renderer, dx12::Dx12Renderer, opengl::OpenglRenderer,
};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: DashMap::default(),
});

pub struct Backends {
    map: DashMap<usize, WindowBackend, FxBuildHasher>,
}

impl Backends {
    pub fn with_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&mut WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let mut backend = BACKENDS.map.entry(hwnd.0 as usize).or_try_insert_with(|| {
            let original_proc: isize =
                unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, hooked_wnd_proc as usize as _) }
                    as isize;
            if original_proc == 0 {
                bail!("SetWindowLongPtrW failed");
            }

            Ok::<_, anyhow::Error>(WindowBackend {
                hwnd: hwnd.0 as _,
                original_proc,

                renderers: Renderers {
                    dx12: None,
                    dx11: None,
                    opengl: None,
                    dx9: None,
                },
            })
        })?;

        Ok(f(&mut backend))
    }

    #[tracing::instrument()]
    pub fn cleanup() {
        BACKENDS.map.clear();
        debug!("backends cleaned up");
    }
}

pub struct WindowBackend {
    hwnd: usize,
    original_proc: isize,

    pub renderers: Renderers,
}

impl Drop for WindowBackend {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(
                HWND(ptr::null_mut::<c_void>().with_addr(self.hwnd)),
                GWLP_WNDPROC,
                self.original_proc as _,
            )
        };
    }
}

pub struct Renderers {
    pub dx12: Option<Dx12Renderer>,
    pub dx11: Option<Dx11Renderer>,
    pub opengl: Option<OpenglRenderer>,
    pub dx9: Option<Dx9Renderer>,
}

#[tracing::instrument]
extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(backend) = BACKENDS.map.get(&(hwnd.0 as usize)) else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    trace!("WNDPROC called for hwnd: {:p}", hwnd.0);

    unsafe {
        CallWindowProcW(
            mem::transmute::<isize, WNDPROC>(backend.original_proc),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}
