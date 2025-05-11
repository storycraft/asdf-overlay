use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use once_cell::sync::OnceCell;
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
    core::{BOOL, PCSTR, s},
};

use crate::{
    app::Overlay,
    backend::{
        Backends,
        opengl::{WglContext, WglContextWrapped},
    },
    renderer::opengl::OpenglRenderer,
    wgl,
};

use super::DetourHook;

struct Hook {
    wgl_swap_buffers: DetourHook<WglSwapBuffersFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

#[tracing::instrument]
pub fn hook() -> anyhow::Result<()> {
    debug!("setting up opengl");
    setup_gl().unwrap();

    let addrs = get_wgl_addrs().expect("cannot get wgl fn addrs");

    HOOK.get_or_try_init(|| unsafe {
        debug!("hooking WglSwapBuffers");
        let wgl_swap_buffers =
            DetourHook::attach(addrs.swap_buffers, hooked_wgl_swap_buffers as _)?;

        Ok::<_, anyhow::Error>(Hook { wgl_swap_buffers })
    })?;

    Ok(())
}

#[tracing::instrument]
unsafe extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    trace!("WglSwapBuffers called");

    let hwnd = unsafe { WindowFromDC(hdc) };
    Backends::with_or_init_backend(hwnd, |backend| {
        if backend.renderer.dx11.is_some() {
            if backend.renderer.opengl.is_some() {
                debug!("Skipping opengl overlay due to dx11 layer");
                backend.renderer.opengl = None;
            }

            return;
        }

        trace!("using opengl renderer");
        let wrapped = backend.renderer.opengl.get_or_insert_with(|| {
            debug!("initializing opengl renderer");
            WglContextWrapped::new_with(
                WglContext::new(hdc).expect("failed to create GlContext"),
                || OpenglRenderer::new().expect("renderer creation failed"),
            )
        });

        wrapped.with(|renderer| {
            let screen = backend.size;
            if let Some(shared) = backend.pending_handle.take() {
                renderer.update_texture(shared);
            }

            let size = renderer.size();
            let position = Overlay::with(|overlay| {
                overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
            });

            let _res = renderer.draw(position, screen);
            trace!("opengl render: {:?}", _res);
        });
    })
    .expect("Backends::with_backend failed");

    let hook = HOOK.get().unwrap();
    unsafe { hook.wgl_swap_buffers.original_fn()(hdc) }
}

type WglSwapBuffersFn = unsafe extern "system" fn(HDC) -> BOOL;

struct WglAddrs {
    swap_buffers: WglSwapBuffersFn,
}

#[tracing::instrument]
fn get_wgl_addrs() -> anyhow::Result<WglAddrs> {
    // Grab a handle to opengl32.dll
    let opengl32module = unsafe { GetModuleHandleA(s!("opengl32.dll"))? };

    let func = unsafe {
        GetProcAddress(opengl32module, s!("wglSwapBuffers")).context("wglSwapBuffers not found")?
    };
    debug!("WglSwapBuffers found: {:p}", func);
    let swap_buffers =
        unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapBuffersFn>(func) };

    Ok(WglAddrs { swap_buffers })
}

#[tracing::instrument]
fn setup_gl() -> anyhow::Result<()> {
    let opengl32module = unsafe { GetModuleHandleA(s!("opengl32.dll"))? };

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
