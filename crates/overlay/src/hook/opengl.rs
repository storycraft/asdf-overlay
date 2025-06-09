use core::{cell::Cell, ffi::c_void, mem};
use std::ffi::CString;

use anyhow::{Context, bail};
use asdf_overlay_common::request::UpdateSharedHandle;
use asdf_overlay_hook::DetourHook;
use once_cell::sync::{Lazy, OnceCell};
use scopeguard::defer;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Gdi::{HDC, WGL_SWAP_MAIN_PLANE, WindowFromDC},
            OpenGL::{HGLRC, wglGetCurrentContext, wglGetProcAddress},
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{BOOL, PCSTR, s},
};

use crate::{
    app::Overlay,
    backend::Backends,
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

// HGLRC -> (HWND, OpenglRenderer)
static MAP: Lazy<IntDashMap<u32, (u32, OpenglRenderer)>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let addrs = get_wgl_addrs().expect("cannot get wgl fn addrs");

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

#[tracing::instrument]
extern "system" fn hooked_wgl_delete_context(hglrc: HGLRC) -> BOOL {
    trace!("wglDeleteContext called");

    cleanup_renderer(hglrc);

    let hook = HOOK.get().unwrap();
    unsafe { hook.wgl_delete_context.original_fn()(hglrc) }
}

#[tracing::instrument]
fn cleanup_renderer(hglrc: HGLRC) {
    debug!("gl renderer cleanup");

    if let Some((_, (hwnd, mut renderer))) = MAP.remove(&(hglrc.0 as u32)) {
        _ = Backends::with_backend(HWND(hwnd as _), |backend| {
            if let Some(handle) = renderer.take_texture() {
                backend.pending_handle = Some(UpdateSharedHandle {
                    handle: Some(handle),
                });
            }
        });
    }
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

#[inline]
fn draw_overlay(hdc: HDC) {
    fn inner(overlay: &Overlay, hglrc: HGLRC, hwnd: HWND) {
        let should_cleanup = Backends::with_or_init_backend(hwnd, |backend| {
            // Disable opengl rendering if presenting on DXGI Swapchain is enabled
            // Nvidia(DX11), AMD(DX12)
            if backend.renderer.dx11.is_some() || backend.renderer.dx12.is_some() {
                return true;
            }

            trace!("using opengl renderer");
            with_renderer_gl_data(|| {
                let (_, ref mut renderer) =
                    *match MAP.entry(hglrc.0 as u32).or_try_insert_with(|| {
                        debug!("initializing opengl renderer");

                        if !gl::GetIntegerv::is_loaded() {
                            debug!("setting up opengl");
                            setup_gl().context("opengl setup failed")?;
                        }

                        if !wgl::DXOpenDeviceNV::is_loaded() {
                            bail!("WGL_NV_DX_interop2 is not supported");
                        }

                        Ok::<_, anyhow::Error>((
                            hwnd.0 as u32,
                            OpenglRenderer::new()
                                .context("failed to create OpenglRenderer")
                                .context("failed to create WglContextWrapped")?,
                        ))
                    }) {
                        Ok(renderer) => renderer,
                        Err(err) => {
                            error!("renderer setup failed. err: {:?}", err);
                            return true;
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
                false
            })
        });

        match should_cleanup {
            Ok(true) => {
                cleanup_renderer(hglrc);
            }
            Ok(_) => {}
            Err(_err) => {
                error!("Backends::with_or_init_backend failed. err: {:?}", _err);
            }
        }
    }

    let hglrc = unsafe { wglGetCurrentContext() };
    if hglrc.is_invalid() {
        return;
    }

    let hwnd = unsafe { WindowFromDC(hdc) };
    if hwnd.is_invalid() {
        return;
    }

    let enabled = Overlay::with(|overlay| {
        inner(overlay, hglrc, hwnd);
    })
    .is_some();

    if !enabled {
        cleanup_renderer(hglrc);
    }
}

#[tracing::instrument]
extern "system" fn hooked_swap_buffers(hdc: HDC) -> BOOL {
    trace!("SwapBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 {
            draw_overlay(hdc);
        }

        let hook = HOOK.get().unwrap();
        unsafe { hook.swap_buffers.original_fn()(hdc) }
    })
}

#[tracing::instrument]
extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    trace!("WglSwapBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 {
            draw_overlay(hdc);
        }

        let hook = HOOK.get().unwrap();
        unsafe { hook.wgl_swap_buffers.original_fn()(hdc) }
    })
}

#[tracing::instrument]
extern "system" fn hooked_wgl_swap_layer_buffers(hdc: HDC, plane: u32) -> BOOL {
    trace!("SwapLayerBuffers called");

    with_gl_call_count(move |last_call_count| {
        if last_call_count == 0 && plane == WGL_SWAP_MAIN_PLANE {
            draw_overlay(hdc);
        }

        let hook = HOOK.get().unwrap();
        unsafe { hook.wgl_swap_layer_buffers.original_fn()(hdc, plane) }
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
