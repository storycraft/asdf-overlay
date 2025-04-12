mod cx;

use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use cx::OverlayGlContext;
use parking_lot::{Mutex, RwLock};
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

static CX: Mutex<Option<OverlayGlContext>> = Mutex::new(None);

unsafe extern "system" fn hooked(hdc: *mut c_void) -> BOOL {
    let Some(ref hook) = *HOOK.read() else {
        return BOOL(0);
    };

    let mut cx = CX.lock();
    let cx = cx.get_or_insert_with(|| OverlayGlContext::new(HDC(hdc)).unwrap());

    cx.with(HDC(hdc), || {
        Renderers::with(|renderers| {
            let renderer = renderers.opengl.get_or_insert_with(|| {
                setup_gl().unwrap();

                OpenglRenderer::new()
            });

            let screen = get_client_size(unsafe { WindowFromDC(HDC(hdc)) }).unwrap_or_default();
            Overlay::with(|overlay| {
                let size = renderer.size();
                renderer.draw(
                    overlay.calc_overlay_position((size.0 as _, size.1 as _), screen),
                    screen,
                );

                unsafe { mem::transmute::<*const (), WglSwapBuffersFn>(hook.original_fn())(hdc) }
            })
        })
    })
}

type WglSwapBuffersFn = unsafe extern "system" fn(*mut c_void) -> BOOL;

static HOOK: RwLock<Option<DetourHook>> = RwLock::new(None);

pub fn hook() -> anyhow::Result<()> {
    if let Ok(wgl_swap_buffers) = get_opengl_wglswapbuffers_addr() {
        let hook = unsafe { DetourHook::attach(wgl_swap_buffers as _, hooked as _)? };
        *HOOK.write() = Some(hook);
    }

    Ok(())
}

pub fn cleanup() {
    HOOK.write().take();
}

fn get_opengl_wglswapbuffers_addr() -> anyhow::Result<WglSwapBuffersFn> {
    // Grab a handle to opengl32.dll
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    let wglswapbuffers = CString::new("wglSwapBuffers")?;
    let func = unsafe {
        GetProcAddress(opengl32module, PCSTR(wglswapbuffers.as_ptr() as *mut _))
            .context("wglSwapBuffers not found")?
    };

    Ok(unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapBuffersFn>(func) })
}

fn setup_gl() -> anyhow::Result<()> {
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    fn loader(module: HMODULE, s: &str) -> *const c_void {
        let name = CString::new(s).unwrap();

        unsafe {
            let addr = PCSTR(name.as_ptr() as _);
            let fn_ptr = wglGetProcAddress(addr);
            if let Some(ptr) = fn_ptr {
                ptr as _
            } else {
                GetProcAddress(module, addr).map_or(std::ptr::null(), |fn_ptr| fn_ptr as *const _)
            }
        }
    }

    wgl::load_with(|s| loader(opengl32module, s));
    gl::load_with(|s| loader(opengl32module, s));

    Ok(())
}
