mod callback;

use core::{cell::Cell, ffi::c_void, ptr};

use anyhow::Context;

use asdf_overlay_event::{Event, SurfaceEvent, SurfaceType};
use asdf_overlay_hook::DetourHook;
use dashmap::Entry;
use once_cell::sync::{Lazy, OnceCell};
use scopeguard::defer;
use tracing::{Level, debug, error, info, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LUID, RECT},
        Graphics::{
            Direct3D9::{
                D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DBACKBUFFER_TYPE_MONO,
                D3DCREATE_HARDWARE_VERTEXPROCESSING, D3DDEVICE_CREATION_PARAMETERS, D3DDEVTYPE_HAL,
                D3DDISPLAYMODEEX, D3DPRESENT_PARAMETERS, D3DSBT_ALL, D3DSWAPEFFECT_DISCARD,
                Direct3DCreate9Ex, IDirect3D9Ex, IDirect3DDevice9, IDirect3DDevice9Ex,
                IDirect3DSurface9, IDirect3DSwapChain9,
            },
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
            Gdi::RGNDATA,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    event_sink::OverlayEventSink,
    hook::dx::dx9::callback::register_destruction_callback,
    interop::DxInterop,
    renderer::dx9::Dx9Renderer,
    surface::{SurfaceState, Surfaces},
    types::IntDashMap,
    util::find_adapter_by_luid,
};

struct Data {
    /// Implicit swapchain renderer key
    main_surface: usize,

    /// Mapping from [`IDirect3DSurface9`] to [`Renderer`]
    renderers: IntDashMap<usize, Renderer>,
}

struct Renderer(Dx9Renderer);

impl Drop for Renderer {
    fn drop(&mut self) {
        info!("Direct3D9 renderer cleanup");
    }
}

/// Mapping from [`IDirect3DDevice9`] to [`Data`]
static MAP: Lazy<IntDashMap<usize, Data>> = Lazy::new(IntDashMap::default);

#[inline]
fn with_or_init_renderer<R>(
    device: &IDirect3DDevice9,
    id: usize,
    f: impl FnOnce(&mut Dx9Renderer) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let key = device.as_raw() as usize;
    let data = match MAP.get(&key) {
        Some(data) => data,
        None => MAP
            .entry(key)
            .or_try_insert_with(|| {
                with_release_disabled(|_| {
                    let main_surface =
                        unsafe { device.GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO) }
                            .context("getting main surface")?
                            .as_raw() as usize;

                    Ok::<_, anyhow::Error>(Data {
                        main_surface,
                        renderers: IntDashMap::default(),
                    })
                })
            })?
            .downgrade(),
    };

    let mut renderer = match data.renderers.entry(id) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            info!("initializing dx9 renderer");
            entry.insert(Renderer(Dx9Renderer::new(device)?))
        }
    };
    f(&mut renderer.0)
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
        present(device, None);
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

    if OverlayEventSink::connected() {
        let swapchain = unsafe { IDirect3DSwapChain9::from_raw_borrowed(&this) }.unwrap();

        // Swapchain still holds reference to device, release owned silently.
        if let Ok(device) = unsafe { swapchain.GetDevice() } {
            let device = device.into_raw();
            let device = unsafe {
                (HOOK.release.wait().original_fn())(device);
                IDirect3DDevice9::from_raw_borrowed(&device).unwrap()
            };

            present(device, Some(swapchain));
        }
    }

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

fn release(device: usize, count: u32) -> Option<u32> {
    if count == 0 {
        MAP.remove(&device);
        return None;
    }

    let renderer_device_count = {
        let data = MAP.get(&device)?;
        data.renderers.get(&data.main_surface)?.0.reference_count()
    };
    if count > renderer_device_count {
        return Some(count - renderer_device_count);
    }

    MAP.remove(&device);
    Some(0)
}

fn with_release_disabled<R>(f: impl FnOnce(bool) -> R) -> R {
    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
    }

    if ENABLED.get() {
        return f(false);
    }

    ENABLED.set(true);
    defer!({
        ENABLED.set(false);
    });
    f(true)
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_release(this: *mut c_void) -> u32 {
    trace!("IDirect3DDevice9::Release called");

    let count = unsafe { HOOK.release.wait().original_fn()(this) };

    with_release_disabled(|locked| {
        if !locked {
            return count;
        }

        release(this as usize, count).unwrap_or(count)
    })
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
        present(device, None);
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

fn draw_overlay(
    device: &IDirect3DDevice9,
    swapchain: Option<&IDirect3DSwapChain9>,
) -> anyhow::Result<()> {
    let dx9_surface = unsafe {
        match swapchain {
            Some(swapchain) => swapchain.GetBackBuffer(0, D3DBACKBUFFER_TYPE_MONO)?,
            None => device.GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO)?,
        }
    };
    let id = dx9_surface.as_raw() as usize;

    Surfaces::with(
        id as u64,
        move || setup_fn(device, swapchain, &dx9_surface),
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
            with_or_init_renderer(device, id, |renderer| {
                trace!("Using Direct3D9 renderer");

                let surface_lock = state.texture.get();
                let Some(surface) = surface_lock.as_ref() else {
                    return Ok(());
                };

                let interop = &state.interop;
                renderer
                    .update_texture(device, surface, &interop.device, &interop.cx.lock())
                    .context("updating renderer texture")?;

                unsafe {
                    device.BeginScene().context("BeginScene failed")?;
                    defer!({
                        _ = device.EndScene();
                    });

                    let state_block = device
                        .CreateStateBlock(D3DSBT_ALL)
                        .context("creating StateBlock failed")?;
                    defer!({
                        _ = state_block.Apply();
                    });

                    renderer.draw(device, position, screen)
                }
            })
        },
    )
}

fn present(device: &IDirect3DDevice9, swapchain: Option<&IDirect3DSwapChain9>) {
    with_release_disabled(move |_| {
        if let Err(err) = draw_overlay(device, swapchain) {
            error!("Failed to draw Direct3D9 overlay. err: {:?}", err);
        }
    });
}

fn cleanup_surface(device: usize, key: usize) {
    with_release_disabled(|_| {
        if !Surfaces::cleanup_state(key as _) {
            return;
        }

        let Some(entry) = MAP.get(&device) else {
            return;
        };

        entry.renderers.remove(&key);
    })
}

fn setup_fn(
    device: &IDirect3DDevice9,
    swapchain: Option<&IDirect3DSwapChain9>,
    surface: &IDirect3DSurface9,
) -> anyhow::Result<SurfaceState> {
    let swapchain = match swapchain {
        Some(swapchain) => swapchain,
        None => &unsafe { device.GetSwapChain(0) }?,
    };
    let mut present_params = D3DPRESENT_PARAMETERS::default();
    unsafe {
        swapchain.GetPresentParameters(&mut present_params)?;
    };

    let window_id = if !present_params.hDeviceWindow.is_invalid() {
        present_params.hDeviceWindow.0 as u32
    } else {
        let mut creation_params = D3DDEVICE_CREATION_PARAMETERS::default();
        unsafe { device.GetCreationParameters(&mut creation_params) }?;
        creation_params.hFocusWindow.0 as u32
    };

    let interop = DxInterop::new(get_dxgi_adapter(device).as_ref())?;

    register_destruction_callback(surface, {
        let device = device.as_raw() as usize;
        let key = surface.as_raw() as usize;
        move || {
            cleanup_surface(device, key);
        }
    })?;
    SurfaceState::new(
        interop,
        (
            present_params.BackBufferWidth,
            present_params.BackBufferHeight,
        ),
        SurfaceType::Direct3D9 { window_id },
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

fn reset(device: &IDirect3DDevice9) {
    // Cleanup device state
    MAP.remove(&(device.as_raw() as _));
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_reset(this: *mut c_void, param: *mut D3DPRESENT_PARAMETERS) -> HRESULT {
    trace!("Reset called");

    let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
    reset(device);

    unsafe { HOOK.reset.wait().original_fn()(this, param) }
}

#[tracing::instrument(level = Level::TRACE)]
extern "system" fn hooked_reset_ex(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
    fullscreen_display_mode: *mut D3DDISPLAYMODEEX,
) -> HRESULT {
    trace!("ResetEx called");

    let device = unsafe { IDirect3DDevice9Ex::from_raw_borrowed(&this) }.unwrap();
    reset(device);

    unsafe { HOOK.reset_ex.wait().original_fn()(this, param, fullscreen_display_mode) }
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
/// IDirect3DDevice9Ex::PresentEx, IDirect3DDevice9::Reset,
/// IDirect3DDevice9Ex::ResetEx by creating dummy device
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
        dx9ex
            .CreateDeviceEx(
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
            )
            .context("cannot create IDirect3DDevice9Ex")?;
        device.unwrap()
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
