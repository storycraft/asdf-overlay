use core::{cell::Cell, ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use dashmap::Entry;
use once_cell::sync::{Lazy, OnceCell};
use scopeguard::defer;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Gdi::{HDC, WGL_SWAP_MAIN_PLANE, WindowFromDC},
            OpenGL::{
                HGLRC, wglGetCurrentContext, wglGetCurrentDC, wglGetProcAddress, wglMakeCurrent,
            },
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{BOOL, PCSTR, s},
};

use crate::{
    app::OverlayIpc,
    backend::{Backends, render::Renderer},
    gl,
    renderer::opengl::{OpenglRenderer, data::with_renderer_gl_data},
    types::IntDashMap,
    wgl,
};

#[link(name = "gdi32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn SwapBuffers(hdc: HDC) -> BOOL;
}

struct Hook {
    wgl_delete_context: DetourHook<WglDeleteContextFn>,
    swap_buffers: DetourHook<SwapBuffersFn>,
    wgl_swap_buffers: DetourHook<WglSwapBuffersFn>,
    wgl_swap_layer_buffers: DetourHook<WglSwapLayerBuffersFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

struct GlData {
    hglrc: u32,
    hwnd: u32,
    renderer: Option<OpenglRenderer>,
}
// HDC -> GlData
static MAP: Lazy<IntDashMap<u32, GlData>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) {
    fn inner() -> anyhow::Result<()> {
        let addrs = get_wgl_addrs().context("failed to load opengl addrs")?;

        HOOK.get_or_try_init(|| unsafe {
            debug!("hooking WglDeleteContext");
            let wgl_delete_context =
                DetourHook::attach(addrs.delete_context, hooked_wgl_delete_context as _)?;

            debug!("hooking SwapBuffers");
            let swap_buffers =
                DetourHook::attach(SwapBuffers as SwapBuffersFn, hooked_swap_buffers as _)?;

            debug!("hooking WglSwapBuffers");
            let wgl_swap_buffers =
                DetourHook::attach(addrs.swap_buffers, hooked_wgl_swap_buffers as _)?;

            debug!("hooking WglSwapLayerBuffers");
            let wgl_swap_layer_buffers =
                DetourHook::attach(addrs.swap_layer_buffers, hooked_wgl_swap_layer_buffers as _)?;

            Ok::<_, anyhow::Error>(Hook {
                wgl_delete_context,
                swap_buffers,
                wgl_swap_buffers,
                wgl_swap_layer_buffers,
            })
        })?;

        Ok(())
    }

    if let Err(err) = inner() {
        error!("failed to hook opengl. err: {err:?}");
    }
}

#[tracing::instrument]
extern "system" fn hooked_wgl_delete_context(hglrc: HGLRC) -> BOOL {
    trace!("wglDeleteContext called");

    let current_hdc = unsafe { wglGetCurrentDC() };
    let current_hglrc = unsafe { wglGetCurrentContext() };
    let mut renderer_cleanup = false;
    MAP.retain(|&hdc, gl_data| {
        if gl_data.hglrc != hglrc.0 as u32 {
            return true;
        }
        if !renderer_cleanup {
            renderer_cleanup = true;
        }

        debug!(
            "gl renderer cleanup hdc: {hdc:x} hwnd: {:x} hglrc: {:x}",
            gl_data.hwnd, gl_data.hglrc
        );
        unsafe {
            _ = wglMakeCurrent(HDC(hdc as _), HGLRC(gl_data.hglrc as _));
            gl_data.renderer.take();
        }
        _ = Backends::with_backend(HWND(gl_data.hwnd as _), |backend| {
            let mut render = backend.render.lock();

            let Some(Renderer::Opengl) = render.renderer else {
                return;
            };
            render.set_surface_updated();
        });

        false
    });
    if renderer_cleanup {
        unsafe {
            _ = wglMakeCurrent(current_hdc, current_hglrc);
        }
    }

    unsafe { HOOK.wait().wgl_delete_context.original_fn()(hglrc) }
}

#[inline]
fn with_gl_call_count<R>(f: impl FnOnce(u32) -> R) -> R {
    thread_local! {
        static SWAP_CALL_COUNT: Cell<u32> = const { Cell::new(0) };
    }

    let last_call_count = SWAP_CALL_COUNT.get();
    SWAP_CALL_COUNT.set(last_call_count + 1);
    defer!({
        SWAP_CALL_COUNT.set(last_call_count);
    });

    f(last_call_count)
}

fn draw_overlay(hdc: HDC) {
    #[inline]
    fn inner(hwnd: HWND, renderer: &mut Option<OpenglRenderer>) {
        let res = Backends::with_or_init_backend(hwnd, |backend| {
            let render = &mut *backend.render.lock();

            match render.renderer {
                Some(Renderer::Opengl) => {}
                Some(_) => {
                    trace!("ignoring opengl rendering");
                    return;
                }
                None => {
                    debug!("Found opengl window");
                    render.renderer = Some(Renderer::Opengl);
                    // wait next swap for possible dxgi swapchain check
                    return;
                }
            }

            if !gl::GetIntegerv::is_loaded() {
                debug!("setting up opengl");
                if let Err(err) = setup_gl() {
                    error!("opengl setup failed. err: {:?}", err);
                    return;
                }
            }

            let renderer = match renderer {
                Some(renderer) => renderer,
                None => {
                    debug!("initializing opengl renderer");

                    renderer.insert(match OpenglRenderer::new(&render.interop.device) {
                        Ok(renderer) => renderer,
                        Err(err) => {
                            error!("renderer setup failed. err: {:?}", err);
                            return;
                        }
                    })
                }
            };
            trace!("using opengl renderer");
            with_renderer_gl_data(|| {
                if render.surface.invalidate_update() {
                    if let Err(err) = renderer
                        .update_texture(render.surface.get().map(|surface| surface.texture()))
                    {
                        error!("failed to update opengl texture. err: {err:?}");
                        return;
                    }
                }
                let Some(surface) = render.surface.get() else {
                    return;
                };

                let _res = renderer.draw(render.position, surface.size(), render.window_size);
                trace!("opengl render: {:?}", _res);
            })
        });

        match res {
            Ok(_) => {}
            Err(_err) => {
                error!("Backends::with_or_init_backend failed. err: {:?}", _err);
            }
        }
    }

    if !OverlayIpc::connected() {
        return;
    }

    let mut data = match MAP.entry(hdc.0 as u32) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            let hglrc = unsafe { wglGetCurrentContext() };
            if hglrc.is_invalid() {
                return;
            }

            let hwnd = unsafe { WindowFromDC(hdc) };
            if hwnd.is_invalid() {
                return;
            }

            entry.insert(GlData {
                hglrc: hglrc.0 as _,
                hwnd: hwnd.0 as _,
                renderer: None,
            })
        }
    };
    inner(HWND(data.hwnd as _), &mut data.renderer);
}

#[tracing::instrument]
extern "system" fn hooked_swap_buffers(hdc: HDC) -> BOOL {
    trace!("SwapBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 {
            draw_overlay(hdc);
        }

        unsafe { HOOK.wait().swap_buffers.original_fn()(hdc) }
    })
}

#[tracing::instrument]
extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    trace!("WglSwapBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 {
            draw_overlay(hdc);
        }

        unsafe { HOOK.wait().wgl_swap_buffers.original_fn()(hdc) }
    })
}

#[tracing::instrument]
extern "system" fn hooked_wgl_swap_layer_buffers(hdc: HDC, plane: u32) -> BOOL {
    trace!("SwapLayerBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 && plane == WGL_SWAP_MAIN_PLANE {
            draw_overlay(hdc);
        }

        unsafe { HOOK.wait().wgl_swap_layer_buffers.original_fn()(hdc, plane) }
    })
}

type SwapBuffersFn = unsafe extern "system" fn(HDC) -> BOOL;
type WglSwapBuffersFn = unsafe extern "system" fn(HDC) -> BOOL;
type WglSwapLayerBuffersFn = unsafe extern "system" fn(HDC, u32) -> BOOL;
type WglDeleteContextFn = unsafe extern "system" fn(HGLRC) -> BOOL;

struct WglAddrs {
    delete_context: WglDeleteContextFn,
    swap_buffers: WglSwapBuffersFn,
    swap_layer_buffers: WglSwapLayerBuffersFn,
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

    let func = unsafe {
        GetProcAddress(opengl32module, s!("wglSwapLayerBuffers"))
            .context("wglSwapLayerBuffers not found")?
    };
    debug!("wglSwapLayerBuffers found: {:p}", func);
    let swap_layer_buffers = unsafe {
        mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapLayerBuffersFn>(func)
    };

    Ok(WglAddrs {
        delete_context,
        swap_buffers,
        swap_layer_buffers,
    })
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
