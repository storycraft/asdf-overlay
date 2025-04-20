mod cx;

use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use cx::OverlayGlContext;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Gdi::{HDC, WindowFromDC},
            OpenGL::wglGetProcAddress,
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{BOOL, PCSTR},
};

use crate::{
    app::Overlay,
    renderer::{Renderers, opengl::OpenglRenderer},
    util::get_client_size,
    wgl,
};

use super::DetourHook;

static CX: OnceCell<OverlayGlContext> = OnceCell::new();

#[tracing::instrument]
unsafe extern "system" fn hooked_wgl_swap_buffers(hdc: *mut c_void) -> BOOL {
    let Some(ref hook) = *HOOK.read() else {
        return BOOL(0);
    };
    trace!("WglSwapBuffers called");

    let cx = CX
        .get_or_try_init(|| OverlayGlContext::new(HDC(hdc)))
        .unwrap();

    cx.with(HDC(hdc), || {
        Renderers::with(|renderers| {
            trace!("using opengl renderer");
            let renderer = renderers.opengl.get_or_insert_with(|| {
                debug!("setting up opengl");
                setup_gl().unwrap();

                debug!("initializing opengl renderer");
                OpenglRenderer::new().expect("renderer creation failed")
            });

            let screen = get_client_size(unsafe { WindowFromDC(HDC(hdc)) }).unwrap_or_default();
            let position = Overlay::with(|overlay| {
                let size = renderer.size();

                if let Some(shared) = overlay.take_pending_handle() {
                    renderer.update_texture(shared);
                }

                overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
            });

            _ = renderer.draw(position, screen);
        })
    });

    unsafe { mem::transmute::<*const (), WglSwapBuffersFn>(hook.original_fn())(hdc) }
}

type WglSwapBuffersFn = unsafe extern "system" fn(*mut c_void) -> BOOL;

static HOOK: RwLock<Option<DetourHook>> = RwLock::new(None);

#[tracing::instrument]
pub fn hook() -> anyhow::Result<()> {
    if let Ok(wgl_swap_buffers) = get_wgl_swap_buffers_addr() {
        debug!("hooking WglSwapBuffers");
        let hook =
            unsafe { DetourHook::attach(wgl_swap_buffers as _, hooked_wgl_swap_buffers as _)? };
        *HOOK.write() = Some(hook);
    }

    Ok(())
}

#[tracing::instrument]
fn get_wgl_swap_buffers_addr() -> anyhow::Result<WglSwapBuffersFn> {
    // Grab a handle to opengl32.dll
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    let wglswapbuffers = CString::new("wglSwapBuffers")?;
    let func = unsafe {
        GetProcAddress(opengl32module, PCSTR(wglswapbuffers.as_ptr() as *mut _))
            .context("wglSwapBuffers not found")?
    };
    debug!("WglSwapBuffers found: {:p}", func);

    Ok(unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapBuffersFn>(func) })
}

#[tracing::instrument]
fn setup_gl() -> anyhow::Result<()> {
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    #[tracing::instrument]
    fn loader(module: HMODULE, s: &str) -> *const c_void {
        let name = CString::new(s).unwrap();

        let addr = unsafe {
            let addr = PCSTR(name.as_ptr() as _);
            let fn_ptr = wglGetProcAddress(addr);
            if let Some(ptr) = fn_ptr {
                ptr as _
            } else {
                GetProcAddress(module, addr).map_or(std::ptr::null(), |fn_ptr| fn_ptr as *const _)
            }
        };
        trace!("found: {:p}", addr);

        addr
    }

    wgl::load_with(|s| loader(opengl32module, s));
    gl::load_with(|s| loader(opengl32module, s));

    Ok(())
}
