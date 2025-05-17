mod util;

use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use once_cell::sync::{Lazy, OnceCell};
use scopeguard::defer;
use tracing::{debug, error, trace};
use util::restore_renderer_gl_state;
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Gdi::{GetDC, HDC, WindowFromDC},
            OpenGL::{
                ChoosePixelFormat, HGLRC, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW, PFD_MAIN_PLANE,
                PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR, SetPixelFormat,
                wglCreateContext, wglDeleteContext, wglGetCurrentContext, wglGetCurrentDC,
                wglGetProcAddress, wglMakeCurrent,
            },
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{BOOL, PCSTR, s},
};

use crate::{
    app::Overlay, backend::Backends, renderer::opengl::OpenglRenderer, types::IntDashMap, wgl,
};

use super::DetourHook;

struct Hook {
    wgl_delete_context: DetourHook<WglDeleteContextFn>,
    wgl_swap_buffers: DetourHook<WglSwapBuffersFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

// HGLRC -> OpenglRenderer
static MAP: Lazy<IntDashMap<u32, OpenglRenderer>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let addrs = get_wgl_addrs().expect("cannot get wgl fn addrs");

    HOOK.get_or_try_init(|| unsafe {
        debug!("setting up opengl");
        setup_gl(dummy_hwnd).unwrap();

        debug!("hooking WglDeleteContext");
        let wgl_delete_context =
            DetourHook::attach(addrs.delete_context, hooked_wgl_delete_context as _)?;

        debug!("hooking WglSwapBuffers");
        let wgl_swap_buffers =
            DetourHook::attach(addrs.swap_buffers, hooked_wgl_swap_buffers as _)?;

        Ok::<_, anyhow::Error>(Hook {
            wgl_delete_context,
            wgl_swap_buffers,
        })
    })?;

    Ok(())
}

#[tracing::instrument]
extern "system" fn hooked_wgl_delete_context(hglrc: HGLRC) -> BOOL {
    trace!("wglDeleteContext called");

    MAP.remove(&(hglrc.0 as u32));

    let hook = HOOK.get().unwrap();
    unsafe { hook.wgl_delete_context.original_fn()(hglrc) }
}

#[tracing::instrument]
unsafe extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    trace!("WglSwapBuffers called");

    let last_hglrc = unsafe { wglGetCurrentContext() };

    let enabled = Overlay::with(|overlay| {
        let hwnd = unsafe { WindowFromDC(hdc) };
        if hwnd.is_invalid() {
            error!("invalid HWND");
            return;
        }

        if last_hglrc.is_invalid() {
            error!("invalid HGLRC");
            return;
        }

        let res = Backends::with_or_init_backend(hwnd, |backend| {
            if backend.renderer.dx11.is_some() {
                MAP.remove(&(last_hglrc.0 as u32));
                return;
            }

            trace!("using opengl renderer");
            restore_renderer_gl_state(|| {
                let mut renderer = match MAP.entry(last_hglrc.0 as u32).or_try_insert_with(|| {
                    debug!("initializing opengl renderer");

                    OpenglRenderer::new()
                        .context("failed to create OpenglRenderer")
                        .context("failed to create WglContextWrapped")
                }) {
                    Ok(renderer) => renderer,
                    Err(err) => {
                        error!("renderer setup failed. err: {:?}", err);
                        return;
                    }
                };

                let screen = backend.size;
                if let Some(shared) = backend.pending_handle.take() {
                    renderer.update_texture(shared);
                }

                let size = renderer.size();
                let position = overlay.calc_overlay_position((size.0 as _, size.1 as _), screen);
                let _res = renderer.draw(position, screen);
                trace!("opengl render: {:?}", _res);
            });
        });

        match res {
            Ok(screen) => screen,
            Err(_err) => {
                error!("Backends::with_or_init_backend failed. err: {:?}", _err);
            }
        }
    })
    .is_some();

    if !enabled {
        MAP.remove(&(last_hglrc.0 as _));
    }

    let hook = HOOK.get().unwrap();
    unsafe { hook.wgl_swap_buffers.original_fn()(hdc) }
}

type WglSwapBuffersFn = unsafe extern "system" fn(HDC) -> BOOL;
type WglDeleteContextFn = unsafe extern "system" fn(HGLRC) -> BOOL;

struct WglAddrs {
    delete_context: WglDeleteContextFn,
    swap_buffers: WglSwapBuffersFn,
}

#[tracing::instrument]
fn get_wgl_addrs() -> anyhow::Result<WglAddrs> {
    // Grab a handle to opengl32.dll
    let opengl32module = unsafe { GetModuleHandleA(s!("opengl32.dll"))? };

    let func = unsafe {
        GetProcAddress(opengl32module, s!("wglDeleteContext"))
            .context("wglDeleteContext not found")?
    };
    debug!("wglDeleteContext found: {:p}", func);
    let delete_context =
        unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglDeleteContextFn>(func) };

    let func = unsafe {
        GetProcAddress(opengl32module, s!("wglSwapBuffers")).context("wglSwapBuffers not found")?
    };
    debug!("WglSwapBuffers found: {:p}", func);
    let swap_buffers =
        unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapBuffersFn>(func) };

    Ok(WglAddrs {
        delete_context,
        swap_buffers,
    })
}

#[tracing::instrument]
fn setup_gl(dummy_hwnd: HWND) -> anyhow::Result<()> {
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

    unsafe {
        let last_hdc = wglGetCurrentDC();
        let last_cx = wglGetCurrentContext();

        let hdc = GetDC(Some(dummy_hwnd));
        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: mem::size_of::<PIXELFORMATDESCRIPTOR>() as _,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: 32,
            cDepthBits: 24,
            cStencilBits: 8,
            iLayerType: PFD_MAIN_PLANE.0 as _,
            ..PIXELFORMATDESCRIPTOR::default()
        };
        let format = ChoosePixelFormat(hdc, &pfd);
        SetPixelFormat(hdc, format, &pfd).context("SetPixelFormat failed")?;

        let tmp_cx = wglCreateContext(hdc).context("wglCreateContext failed")?;
        defer!({
            _ = wglDeleteContext(tmp_cx);
        });

        wglMakeCurrent(hdc, tmp_cx).context("wglMakeCurrent failed")?;

        wgl::load_with(|s| loader(opengl32module, s));
        gl::load_with(|s| loader(opengl32module, s));

        wglMakeCurrent(last_hdc, last_cx).context("wglMakeCurrent failed")?;
    }

    Ok(())
}
