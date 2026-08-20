use core::{ffi::c_void, ptr};
use std::thread;

use anyhow::Context;
use asdf_overlay_event::{Event, SurfaceEvent, SurfaceInfo, SurfaceType};
use asdf_overlay_hook::DetourHook;
use dashmap::Entry;
use once_cell::sync::{Lazy, OnceCell};
use tracing::{Level, debug, error, info, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LUID, RECT},
        Graphics::{
            Direct3D9::{
                D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DBACKBUFFER_TYPE_MONO,
                D3DCREATE_HARDWARE_VERTEXPROCESSING, D3DDEVICE_CREATION_PARAMETERS, D3DDEVTYPE_HAL,
                D3DDISPLAYMODEEX, D3DPRESENT_PARAMETERS, D3DSURFACE_DESC, D3DSWAPEFFECT_DISCARD,
                Direct3DCreate9Ex, IDirect3D9Ex, IDirect3DDevice9, IDirect3DSwapChain9,
            },
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
            Gdi::RGNDATA,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    event_sink::OverlayEventSink,
    interop::DxInterop,
    renderer::dx9::Dx9Renderer,
    surface::{SurfaceState, Surfaces},
    types::IntDashMap,
    util::find_adapter_by_luid,
};

/// Mapping from [`IDirect3DDevice9`] to [`Dx9Renderer`].
static RENDERERS: Lazy<IntDashMap<usize, Dx9Renderer>> = Lazy::new(IntDashMap::default);

#[inline]
fn with_or_init_renderer<R>(
    device: &IDirect3DDevice9,
    f: impl FnOnce(&mut Dx9Renderer) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let mut data = match RENDERERS.entry(device.as_raw() as _) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            info!("initializing dx9 renderer");
            entry.insert(Dx9Renderer::new(device)?)
        }
    };

    f(&mut data)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_present(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
) -> HRESULT {
    trace!("IDirect3DDevice9::Present called");

    if OverlayEventSink::connected() {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
        let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();
        present(device, &swapchain);
    }

    unsafe {
        HOOK.present.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
        )
    }
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_swapchain_present(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
    dw_flags: u32,
) -> HRESULT {
    trace!("IDirect3DSwapChain9::Present called");

    let swapchain = unsafe { IDirect3DSwapChain9::from_raw_borrowed(&this) }.unwrap();
    let device = unsafe { swapchain.GetDevice() }.unwrap();
    present(&device, swapchain);

    unsafe {
        HOOK.swapchain_present.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
            dw_flags,
        )
    }
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_release(this: *mut c_void) -> u32 {
    trace!("IDirect3DDevice9::Release called");

    let count = unsafe { HOOK.release.wait().original_fn()(this) };

    // renderer includes refs from IDirect3DVertexBuffer9, IDirect3DStateBlock9 and optionally texture.
    if count == 2 || count == 3 {
        reset_renderer(this as _);
        cleanup_renderer(this as _);
    }

    count
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_present_ex(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
    dw_flags: u32,
) -> HRESULT {
    trace!("IDirect3DDevice9Ex::PresentEx called");

    if OverlayEventSink::connected() {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
        let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();
        present(device, &swapchain);
    }

    unsafe {
        HOOK.present_ex.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
            dw_flags,
        )
    }
}

fn draw_overlay(device: &IDirect3DDevice9, swapchain: &IDirect3DSwapChain9) -> anyhow::Result<()> {
    // Use device pointer as key.
    let id = device.as_raw() as u64;

    Surfaces::with(
        id,
        || setup_fn(device, swapchain),
        |state| {
            match state.info.api {
                SurfaceType::Direct3D9 { .. } => {}
                _ => {
                    trace!("ignoring Direct3D9 rendering");
                    return Ok(());
                }
            }

            let position = state.position();
            let screen = state.size();
            with_or_init_renderer(device, |renderer| {
                trace!("Using Direct3D9 renderer");

                let surface_lock = state.texture.get();
                let Some(surface) = surface_lock.as_ref() else {
                    return Ok(());
                };

                let interop = &state.interop;
                renderer
                    .update_texture(device, surface, &interop.device, &interop.cx.lock())
                    .context("updating renderer texture")?;

                unsafe { device.BeginScene() }.context("BeginScene failed")?;
                renderer.draw(device, position, screen)?;
                unsafe { device.EndScene() }.context("EndScene failed")?;

                Ok(())
            })
        },
    )
}

fn present(device: &IDirect3DDevice9, swapchain: &IDirect3DSwapChain9) {
    if let Err(err) = draw_overlay(device, swapchain) {
        error!("Failed to draw Direct3D9 overlay. err: {:?}", err);
    }
}

fn post_reset(device: &IDirect3DDevice9) {
    let id = device.as_raw() as _;

    Surfaces::state(id, |state| {
        let default_swapchain = unsafe { device.GetSwapChain(0) }.unwrap();
        let back_buffer =
            unsafe { default_swapchain.GetBackBuffer(0, D3DBACKBUFFER_TYPE_MONO) }.unwrap();
        let mut desc = D3DSURFACE_DESC::default();
        unsafe { back_buffer.GetDesc(&mut desc) }.unwrap();

        state.texture.invalidate();
        state.resize(desc.Width, desc.Height);
        OverlayEventSink::emit(Event::Surface {
            id: device.as_raw() as _,
            event: SurfaceEvent::Resized {
                width: desc.Width,
                height: desc.Height,
            },
        });
    });
}

fn cleanup_renderer(device: usize) {
    info!("Direct3D9 renderer cleanup");

    // HACK:: workaround for recursive lock
    thread::spawn(move || {
        Surfaces::cleanup_state(device as _);
    });
}

fn reset_renderer(device: usize) {
    info!("Direct3D9 renderer reset");
    if RENDERERS.remove(&device).is_none() {
        return;
    };

    Surfaces::state(device as _, |state| {
        state.texture.invalidate();
    });
}

fn setup_fn(
    device: &IDirect3DDevice9,
    swapchain: &IDirect3DSwapChain9,
) -> anyhow::Result<SurfaceState> {
    let back_buffer = unsafe { swapchain.GetBackBuffer(0, D3DBACKBUFFER_TYPE_MONO) }
        .context("failed to get back buffer")?;

    let mut desc = D3DSURFACE_DESC::default();
    unsafe {
        back_buffer.GetDesc(&mut desc)?;
    }

    let mut present_params = D3DPRESENT_PARAMETERS::default();
    unsafe { swapchain.GetPresentParameters(&mut present_params) }?;

    let window_id = if !present_params.hDeviceWindow.is_invalid() {
        present_params.hDeviceWindow.0 as u32
    } else {
        let mut creation_params = D3DDEVICE_CREATION_PARAMETERS::default();
        unsafe { device.GetCreationParameters(&mut creation_params) }?;
        creation_params.hFocusWindow.0 as u32
    };

    let interop = DxInterop::new(get_dxgi_adapter(device).as_ref())?;
    let gpu_id = interop.gpu_id;
    SurfaceState::new(
        interop,
        (desc.Width, desc.Height),
        SurfaceInfo {
            api: SurfaceType::Direct3D9 { window_id },
            gpu_id,
        },
    )
}

fn get_dxgi_adapter(device: &IDirect3DDevice9) -> Option<IDXGIAdapter> {
    let d3d9ex = unsafe { device.GetDirect3D() }
        .ok()?
        .cast::<IDirect3D9Ex>()
        .ok()?;
    let factory = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }.ok()?;

    let mut param = D3DDEVICE_CREATION_PARAMETERS::default();
    unsafe { device.GetCreationParameters(&mut param) }.ok()?;

    let mut luid = LUID::default();
    unsafe { d3d9ex.GetAdapterLUID(param.AdapterOrdinal, &mut luid) }.ok()?;

    find_adapter_by_luid(&factory, luid)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_reset(this: *mut c_void, param: *mut D3DPRESENT_PARAMETERS) -> HRESULT {
    trace!("Reset called");
    reset_renderer(this as _);

    let res = unsafe { HOOK.reset.wait().original_fn()(this, param) };
    if res.is_err() {
        return res;
    }

    post_reset(unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap());
    res
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_reset_ex(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
    fullscreen_display_mode: *mut D3DDISPLAYMODEEX,
) -> HRESULT {
    trace!("ResetEx called");
    reset_renderer(this as _);

    let res = unsafe { HOOK.reset_ex.wait().original_fn()(this, param, fullscreen_display_mode) };
    if res.is_err() {
        return res;
    }

    post_reset(unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap());
    res
}

type PresentFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
) -> HRESULT;
type ReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;
type PresentExFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
    u32,
) -> HRESULT;
type SwapchainPresentFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
    u32,
) -> HRESULT;
type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;
type ResetExFn = unsafe extern "system" fn(
    *mut c_void,
    *mut D3DPRESENT_PARAMETERS,
    *mut D3DDISPLAYMODEEX,
) -> HRESULT;

struct Hook {
    present: OnceCell<DetourHook<PresentFn>>,
    release: OnceCell<DetourHook<ReleaseFn>>,
    present_ex: OnceCell<DetourHook<PresentExFn>>,
    swapchain_present: OnceCell<DetourHook<SwapchainPresentFn>>,
    reset: OnceCell<DetourHook<ResetFn>>,
    reset_ex: OnceCell<DetourHook<ResetExFn>>,
}

static HOOK: Hook = Hook {
    present: OnceCell::new(),
    release: OnceCell::new(),
    present_ex: OnceCell::new(),
    swapchain_present: OnceCell::new(),
    reset: OnceCell::new(),
    reset_ex: OnceCell::new(),
};

pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let (present, release, swapchain_present, present_ex, reset, reset_ex) =
        get_addr(dummy_hwnd).context("failed to load dx9 addrs")?;

    debug!("hooking IDirect3DDevice9::Reset");
    HOOK.reset
        .get_or_try_init(|| unsafe { DetourHook::attach(reset, hooked_reset as _) })?;
    debug!("hooking IDirect3DDevice9::Release");
    HOOK.release
        .get_or_try_init(|| unsafe { DetourHook::attach(release, hooked_release as _) })?;
    debug!("hooking IDirect3DDevice9Ex::ResetEx");
    HOOK.reset_ex
        .get_or_try_init(|| unsafe { DetourHook::attach(reset_ex, hooked_reset_ex as _) })?;
    debug!("hooking IDirect3DDevice9::Present");
    HOOK.present
        .get_or_try_init(|| unsafe { DetourHook::attach(present, hooked_present as _) })?;
    debug!("hooking IDirect3DSwapChain9::Present");
    HOOK.swapchain_present.get_or_try_init(|| unsafe {
        DetourHook::attach(swapchain_present, hooked_swapchain_present as _)
    })?;
    debug!("hooking IDirect3DDevice9Ex::PresentEx");
    HOOK.present_ex
        .get_or_try_init(|| unsafe { DetourHook::attach(present_ex, hooked_present_ex as _) })?;

    Ok(())
}

/// Get pointer to IDirect3DDevice9::Present, IDirect3DDevice9::Release, IDirect3DSwapChain9::Present,
/// IDirect3DDevice9Ex::PresentEx, IDirect3DDevice9::Reset, IDirect3DDevice9Ex::ResetEx by creating dummy device
fn get_addr(
    dummy_hwnd: HWND,
) -> anyhow::Result<(
    PresentFn,
    ReleaseFn,
    SwapchainPresentFn,
    PresentExFn,
    ResetFn,
    ResetExFn,
)> {
    let device = unsafe {
        let dx9ex = Direct3DCreate9Ex(D3D_SDK_VERSION).context("cannot create IDirect3D9")?;

        let mut device = None;
        dx9ex.CreateDeviceEx(
            D3DADAPTER_DEFAULT,
            D3DDEVTYPE_HAL,
            HWND(ptr::null_mut()),
            D3DCREATE_HARDWARE_VERTEXPROCESSING as _,
            &mut D3DPRESENT_PARAMETERS {
                Windowed: BOOL(1),
                SwapEffect: D3DSWAPEFFECT_DISCARD,
                hDeviceWindow: dummy_hwnd,
                ..Default::default()
            },
            0 as _,
            &mut device,
        )?;

        device.context("cannot create IDirect3DDevice9")?
    };

    let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();

    let vtable = Interface::vtable(&*device);
    let present = vtable.Present;
    debug!("IDirect3DDevice9::Present found: {:p}", present);

    let release = vtable.base__.Release;
    debug!("IDirect3DDevice9::Release found: {:p}", release);

    let swapchain_vtable = Interface::vtable(&swapchain);
    let swapchain_present = swapchain_vtable.Present;
    debug!(
        "IDirect3DSwapChain9::Present found: {:p}",
        swapchain_present
    );

    let reset = vtable.Reset;
    debug!("IDirect3DDevice9::Reset found: {:p}", reset);

    let dx9ex_vtable = Interface::vtable(&device);
    let reset_ex = dx9ex_vtable.ResetEx;
    debug!("IDirect3DDevice9Ex::ResetEx found: {:p}", reset_ex);

    let present_ex = dx9ex_vtable.PresentEx;
    debug!("IDirect3DDevice9Ex::PresentEx found: {:p}", present_ex);

    Ok((
        present,
        release,
        swapchain_present,
        present_ex,
        reset,
        reset_ex,
    ))
}
