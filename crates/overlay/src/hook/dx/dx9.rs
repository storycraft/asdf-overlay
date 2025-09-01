use core::{ffi::c_void, ptr};

use anyhow::Context;
use asdf_overlay_hook::DetourHook;
use once_cell::sync::OnceCell;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HWND, LUID, RECT},
        Graphics::{
            Direct3D9::{
                D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DCREATE_HARDWARE_VERTEXPROCESSING,
                D3DDEVICE_CREATION_PARAMETERS, D3DDEVTYPE_HAL, D3DDISPLAYMODEEX,
                D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_DISCARD, Direct3DCreate9Ex, IDirect3D9Ex,
                IDirect3DDevice9, IDirect3DSwapChain9,
            },
            Dxgi::{CreateDXGIFactory1, IDXGIFactory1},
            Gdi::RGNDATA,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    backend::{Backends, render::Renderer},
    event_sink::OverlayEventSink,
    renderer::dx9::Dx9Renderer,
    util::find_adapter_by_luid,
};

#[tracing::instrument]
extern "system" fn hooked_present(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
) -> HRESULT {
    trace!("Present called");

    if OverlayEventSink::connected() {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
        let mut hwnd = dest_window_override;
        if hwnd.is_invalid() {
            let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();
            let mut params = D3DPRESENT_PARAMETERS::default();
            unsafe { swapchain.GetPresentParameters(&mut params) }.unwrap();
            hwnd = params.hDeviceWindow;
        }
        if !hwnd.is_invalid() {
            draw_overlay(hwnd, device);
        }
    }

    unsafe {
        HOOK.dx9_present.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
        )
    }
}

#[tracing::instrument]
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
    let mut hwnd = dest_window_override;
    if hwnd.is_invalid() {
        let mut params = D3DPRESENT_PARAMETERS::default();
        unsafe { swapchain.GetPresentParameters(&mut params) }.unwrap();
        hwnd = params.hDeviceWindow;
    }
    if !hwnd.is_invalid() {
        draw_overlay(hwnd, &device);
    }

    unsafe {
        HOOK.dx9_swapchain_present.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
            dw_flags,
        )
    }
}

#[tracing::instrument]
extern "system" fn hooked_present_ex(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
    dw_flags: u32,
) -> HRESULT {
    trace!("PresentEx called");

    if OverlayEventSink::connected() {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
        let mut hwnd = dest_window_override;
        if hwnd.is_invalid() {
            let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();
            let mut params = D3DPRESENT_PARAMETERS::default();
            unsafe { swapchain.GetPresentParameters(&mut params) }.unwrap();
            hwnd = params.hDeviceWindow;
        }
        if !hwnd.is_invalid() {
            draw_overlay(hwnd, device);
        }
    }

    unsafe {
        HOOK.dx9_present_ex.wait().original_fn()(
            this,
            source_rect,
            dest_rect,
            dest_window_override,
            dirty_region,
            dw_flags,
        )
    }
}

fn draw_overlay(hwnd: HWND, device: &IDirect3DDevice9) {
    let res = Backends::with_or_init_backend(
        hwnd.0 as _,
        || {
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
        },
        |backend| {
            let render = &mut *backend.render.lock();
            let renderer = match render.renderer {
                Some(Renderer::Dx9(ref mut renderer)) => renderer,
                Some(_) => {
                    trace!("ignoring dx9 rendering");
                    return;
                }
                None => {
                    debug!("Found dx9 window");
                    render.renderer = Some(Renderer::Dx9(None));
                    // wait next swap for possible remaining renderer check
                    return;
                }
            };
            trace!("using dx9 renderer");

            // renderer device might be changed, check with previous device
            let renderer = match *renderer {
                Some((renderer_device, ref mut renderer))
                    if renderer_device == device.as_raw() as _ =>
                {
                    renderer
                }
                _ => {
                    &mut renderer
                        .insert((
                            device.as_raw() as _,
                            Dx9Renderer::new(device).expect("Dx9Renderer creation failed"),
                        ))
                        .1
                }
            };

            if render.surface.invalidate_update() && render.surface.get().is_none() {
                renderer.reset_texture();
            }
            let Some(surface) = render.surface.get() else {
                return;
            };

            let interop = &mut render.interop;
            match renderer.update_texture(
                device,
                surface.size(),
                &interop.device,
                interop.cx.get_mut(),
                surface.texture(),
                surface.mutex(),
            ) {
                Ok(_) => {
                    if unsafe { device.BeginScene() }.is_err() {
                        return;
                    }

                    let _res = renderer.draw(device, render.position, render.window_size);
                    trace!("dx9 render: {:?}", _res);
                    unsafe { _ = device.EndScene() };
                }
                Err(err) => {
                    error!("failed to update dx9 texture. err: {err:?}");
                }
            }
        },
    );

    if let Err(_err) = res {
        error!("Backends::with_or_init_backend failed. err: {:?}", _err);
    }
}

fn handle_reset(device: &IDirect3DDevice9, param: *mut D3DPRESENT_PARAMETERS) {
    let mut hwnd = unsafe { &*param }.hDeviceWindow;
    // hwnd is hDeviceWindow of new param or focus window
    if hwnd.is_invalid() {
        let mut params = D3DDEVICE_CREATION_PARAMETERS::default();
        _ = unsafe { device.GetCreationParameters(&mut params) };
        hwnd = params.hFocusWindow;
    }

    if !hwnd.is_invalid() {
        _ = Backends::with_backend(hwnd.0 as _, |backend| {
            let render = &mut *backend.render.lock();
            let Some(Renderer::Dx9(ref mut renderer)) = render.renderer else {
                return;
            };
            debug!("dx9 renderer cleanup");

            renderer.take();
            render.set_surface_updated();
        });
    }
}

#[tracing::instrument]
extern "system" fn hooked_reset(this: *mut c_void, param: *mut D3DPRESENT_PARAMETERS) -> HRESULT {
    trace!("Reset called");
    handle_reset(
        unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap(),
        param,
    );

    unsafe { HOOK.reset.wait().original_fn()(this, param) }
}

#[tracing::instrument]
extern "system" fn hooked_reset_ex(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
    fullscreen_display_mode: *mut D3DDISPLAYMODEEX,
) -> HRESULT {
    trace!("ResetEx called");
    handle_reset(
        unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap(),
        param,
    );

    unsafe { HOOK.reset_ex.wait().original_fn()(this, param, fullscreen_display_mode) }
}

type PresentFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
) -> HRESULT;
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
type SwapchainReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;
type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;
type ResetExFn = unsafe extern "system" fn(
    *mut c_void,
    *mut D3DPRESENT_PARAMETERS,
    *mut D3DDISPLAYMODEEX,
) -> HRESULT;

struct Hook {
    dx9_present: OnceCell<DetourHook<PresentFn>>,
    dx9_present_ex: OnceCell<DetourHook<PresentExFn>>,
    dx9_swapchain_present: OnceCell<DetourHook<SwapchainPresentFn>>,
    reset: OnceCell<DetourHook<ResetFn>>,
    reset_ex: OnceCell<DetourHook<ResetExFn>>,
}

static HOOK: Hook = Hook {
    dx9_present: OnceCell::new(),
    dx9_present_ex: OnceCell::new(),
    dx9_swapchain_present: OnceCell::new(),
    reset: OnceCell::new(),
    reset_ex: OnceCell::new(),
};

pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let (present, swapchain_present, swapchain_release, present_ex, reset, reset_ex) =
        get_addr(dummy_hwnd).context("failed to load dx9 addrs")?;

    debug!("hooking IDirect3DDevice9::Reset");
    HOOK.reset
        .get_or_try_init(|| unsafe { DetourHook::attach(reset, hooked_reset as _) })?;
    debug!("hooking IDirect3DDevice9Ex::ResetEx");
    HOOK.reset_ex
        .get_or_try_init(|| unsafe { DetourHook::attach(reset_ex, hooked_reset_ex as _) })?;
    debug!("hooking IDirect3DDevice9::Present");
    HOOK.dx9_present
        .get_or_try_init(|| unsafe { DetourHook::attach(present, hooked_present as _) })?;
    debug!("hooking IDirect3DSwapChain9::Present");
    HOOK.dx9_swapchain_present.get_or_try_init(|| unsafe {
        DetourHook::attach(swapchain_present, hooked_swapchain_present as _)
    })?;
    debug!("hooking IDirect3DDevice9Ex::PresentEx");
    HOOK.dx9_present_ex
        .get_or_try_init(|| unsafe { DetourHook::attach(present_ex, hooked_present_ex as _) })?;

    Ok(())
}

/// Get pointer to IDirect3DDevice9::Present, IDirect3DSwapChain9::Present, IDirect3DSwapChain9::Release,
/// IDirect3DDevice9Ex::PresentEx, IDirect3DDevice9::Reset, IDirect3DDevice9Ex::ResetEx by creating dummy device
fn get_addr(
    dummy_hwnd: HWND,
) -> anyhow::Result<(
    PresentFn,
    SwapchainPresentFn,
    SwapchainReleaseFn,
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

    let dx9_vtable = Interface::vtable(&*device);
    let present = dx9_vtable.Present;
    debug!("IDirect3DDevice9::Present found: {:p}", present);

    let dx9_swapchain_vtable = Interface::vtable(&swapchain);
    let swapchain_present = dx9_swapchain_vtable.Present;
    debug!(
        "IDirect3DSwapChain9::Present found: {:p}",
        swapchain_present
    );

    let swapchain_release = dx9_swapchain_vtable.base__.Release;
    debug!(
        "IDirect3DSwapChain9::Release found: {:p}",
        swapchain_release
    );

    let reset = dx9_vtable.Reset;
    debug!("IDirect3DDevice9::Reset found: {:p}", reset);

    let dx9ex_vtable = Interface::vtable(&device);
    let reset_ex = dx9ex_vtable.ResetEx;
    debug!("IDirect3DDevice9Ex::ResetEx found: {:p}", reset_ex);

    let present_ex = dx9ex_vtable.PresentEx;
    debug!("IDirect3DDevice9Ex::PresentEx found: {:p}", present_ex);

    Ok((
        present,
        swapchain_present,
        swapchain_release,
        present_ex,
        reset,
        reset_ex,
    ))
}
