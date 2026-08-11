mod data;

use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use once_cell::sync::{Lazy, OnceCell};
use tracing::{debug, error, info, trace};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND, LUID},
        Graphics::{
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
            Gdi::{HDC, WindowFromDC},
            OpenGL::{
                HGLRC, wglGetCurrentContext, wglGetCurrentDC, wglGetProcAddress, wglMakeCurrent,
            },
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{BOOL, PCSTR, s},
};

use crate::{
    event_sink::OverlayEventSink,
    gl,
    hook::opengl::data::with_renderer_gl_data,
    renderer::opengl::OpenglRenderer,
    surface::{Renderer, SurfaceState, Surfaces},
    types::IntDashMap,
    util::{find_adapter_by_luid, get_client_size},
    wgl,
};

struct Hook {
    wgl_delete_context: DetourHook<WglDeleteContextFn>,
    wgl_swap_buffers: DetourHook<WglSwapBuffersFn>,
}

static HOOK: OnceCell<Hook> = OnceCell::new();

struct GlData {
    hglrc: usize,
    renderer: Option<OpenglRenderer>,
}
// hdc -> GlData
static MAP: Lazy<IntDashMap<u64, GlData>> = Lazy::new(IntDashMap::default);

#[tracing::instrument]
pub fn hook(dummy_hwnd: HWND) {
    fn inner() -> anyhow::Result<()> {
        let addrs = get_wgl_addrs().context("failed to load opengl addrs")?;

        HOOK.get_or_try_init(|| unsafe {
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
    MAP.retain(|&key, gl_data| {
        if gl_data.hglrc != hglrc.0 as usize {
            return true;
        }
        if !renderer_cleanup {
            renderer_cleanup = true;
        }

        info!(
            "gl renderer cleanup hdc: {key:x} hglrc: {:x}",
            gl_data.hglrc
        );
        Surfaces::cleanup_state(key);
        unsafe {
            _ = wglMakeCurrent(HDC(key as _), HGLRC(gl_data.hglrc as _));
        }
        false
    });

    if renderer_cleanup {
        unsafe {
            _ = wglMakeCurrent(current_hdc, current_hglrc);
        }
    }

    unsafe { HOOK.wait().wgl_delete_context.original_fn()(hglrc) }
}

fn draw_overlay(hdc: HDC) {
    #[inline]
    fn inner(state: &SurfaceState, renderer: &mut Option<OpenglRenderer>) {
        if state.renderer != Renderer::Opengl {
            trace!("ignoring opengl rendering");
            return;
        }

        trace!("using opengl renderer");
        with_renderer_gl_data(|| {
            let renderer = match renderer {
                Some(renderer) => renderer,
                None => {
                    info!("initializing opengl renderer");
                    renderer.insert(match OpenglRenderer::new() {
                        Ok(renderer) => renderer,
                        Err(err) => {
                            error!("renderer setup failed. err: {:?}", err);
                            return;
                        }
                    })
                }
            };

            let Some(surface_size) = state.surface_size() else {
                return;
            };

            let position = state.position();
            let screen = state.size();
            if state.surface.invalidate_update()
                && let Err(err) = renderer.update_texture(
                    state
                        .surface
                        .get()
                        .as_ref()
                        .map(|surface| surface.texture()),
                )
            {
                error!("failed to update opengl texture. err: {err:?}");
                return;
            }

            let _res = renderer.draw(position, surface_size, screen);
            trace!("opengl render: {:?}", _res);
        })
    }

    if !OverlayEventSink::connected() {
        return;
    }

    if !gl::GetIntegerv::is_loaded() {
        debug!("setting up opengl");
        if let Err(err) = setup_gl() {
            error!("opengl setup failed. err: {:?}", err);
            return;
        }
    }

    let key = hdc.0 as u64;
    let mut data = {
        if let Some(r) = MAP.get_mut(&key) {
            r
        } else {
            MAP.entry(key).or_insert(GlData {
                hglrc: 0,
                renderer: None,
            })
        }
    };
    data.hglrc = unsafe { wglGetCurrentContext() }.0 as usize;

    let res = Surfaces::with(
        key,
        || setup_fn(unsafe { WindowFromDC(hdc) }),
        |backend| inner(backend, &mut data.renderer),
    );
    match res {
        Ok(_) => {}
        Err(_err) => {
            error!("Backends::with_or_init_backend failed. err: {:?}", _err);
        }
    }
}

fn setup_fn(hwnd: HWND) -> anyhow::Result<SurfaceState> {
    let size = get_client_size(hwnd).unwrap_or_default();
    SurfaceState::new(get_dxgi_adapter().as_ref(), size, Renderer::Opengl)
}

#[tracing::instrument]
extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    trace!("WglSwapBuffers called");

    draw_overlay(hdc);

    unsafe { HOOK.wait().wgl_swap_buffers.original_fn()(hdc) }
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

fn get_dxgi_adapter() -> Option<IDXGIAdapter> {
    let mut luid = LUID::default();
    unsafe {
        _ = gl::GetError();
        gl::GetUnsignedBytevEXT(gl::DEVICE_LUID_EXT, &mut luid as *mut _ as _);
        if gl::GetError() != gl::NO_ERROR {
            return None;
        }
    }

    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>().ok()? };
    find_adapter_by_luid(&factory, luid)
}
