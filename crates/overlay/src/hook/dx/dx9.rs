use core::{ffi::c_void, ptr};

use anyhow::Context;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{
            Direct3D9::{
                D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DCREATE_HARDWARE_VERTEXPROCESSING,
                D3DDEVICE_CREATION_PARAMETERS, D3DDEVTYPE_HAL, D3DDISPLAYMODEEX,
                D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_DISCARD, Direct3DCreate9Ex, IDirect3DDevice9,
                IDirect3DSwapChain9,
            },
            Gdi::RGNDATA,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    backend::{Backends, render::Renderer},
    reader::SharedHandleReader,
    renderer::dx9::Dx9Renderer,
};

use super::HOOK;

pub type PresentFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
) -> HRESULT;
pub type PresentExFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
    u32,
) -> HRESULT;
pub type SwapchainPresentFn = unsafe extern "system" fn(
    *mut c_void,
    *const RECT,
    *const RECT,
    HWND,
    *const RGNDATA,
    u32,
) -> HRESULT;
pub type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;
pub type ResetExFn = unsafe extern "system" fn(
    *mut c_void,
    *mut D3DPRESENT_PARAMETERS,
    *mut D3DDISPLAYMODEEX,
) -> HRESULT;

#[tracing::instrument]
pub(super) extern "system" fn hooked_present(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
) -> HRESULT {
    trace!("Present called");

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
pub(super) extern "system" fn hooked_swapchain_present(
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
pub(super) extern "system" fn hooked_present_ex(
    this: *mut c_void,
    source_rect: *const RECT,
    dest_rect: *const RECT,
    dest_window_override: HWND,
    dirty_region: *const RGNDATA,
    dw_flags: u32,
) -> HRESULT {
    trace!("PresentEx called");

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
    let res = Backends::with_or_init_backend(hwnd, |backend| {
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
        let renderer = renderer
            .get_or_insert_with(|| Dx9Renderer::new(device).expect("Dx9Renderer creation failed"));

        let reader = render
            .cx
            .fallback_reader
            .get_or_insert_with(|| SharedHandleReader::new().unwrap());

        if render.surface.invalidate_update() && render.surface.get().is_none() {
            renderer.reset_texture();
        }
        let Some(surface) = render.surface.get() else {
            return;
        };

        let surface_size = surface.size();
        let interop = &mut render.interop;
        match reader.with_mapped(
            &interop.device,
            surface.mutex(),
            interop.cx.get_mut(),
            surface.texture(),
            surface_size,
            |mapped| renderer.update_texture(device, surface_size, mapped),
        ) {
            Ok(Some(_)) => {
                if unsafe { device.BeginScene() }.is_err() {
                    return;
                }

                let _res = renderer.draw(device, render.position, render.window_size);
                trace!("dx9 render: {:?}", _res);
                unsafe { _ = device.EndScene() };
            }
            Ok(None) => {}
            Err(err) => {
                error!("failed to copy shtex to dx9 texture. err: {err:?}");
            }
        }
    });

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
        _ = Backends::with_backend(hwnd, |backend| {
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
pub(super) extern "system" fn hooked_reset(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
) -> HRESULT {
    trace!("Reset called");
    handle_reset(
        unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap(),
        param,
    );

    unsafe { HOOK.reset.wait().original_fn()(this, param) }
}

#[tracing::instrument]
pub(super) extern "system" fn hooked_reset_ex(
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

/// Get pointer to IDirect3DDevice9::Present, IDirect3DSwapChain9::Present,
/// IDirect3DDevice9Ex::PresentEx, IDirect3DDevice9::Reset, IDirect3DDevice9Ex::ResetEx by creating dummy device
pub fn get_dx9_addr(
    dummy_hwnd: HWND,
) -> anyhow::Result<(
    PresentFn,
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

    let dx9_vtable = Interface::vtable(&*device);
    let present = dx9_vtable.Present;
    debug!("IDirect3DDevice9::Present found: {:p}", present);

    let dx9_swapchain_vtable = Interface::vtable(&swapchain);
    let swapchain_present = dx9_swapchain_vtable.Present;
    debug!(
        "IDirect3DSwapChain9::Present found: {:p}",
        swapchain_present
    );

    let reset = dx9_vtable.Reset;
    debug!("IDirect3DDevice9::Reset found: {:p}", reset);

    let dx9ex_vtable = Interface::vtable(&device);
    let reset_ex = dx9ex_vtable.ResetEx;
    debug!("IDirect3DDevice9Ex::ResetEx found: {:p}", reset_ex);

    let present_ex = dx9ex_vtable.PresentEx;
    debug!("IDirect3DDevice9Ex::PresentEx found: {:p}", present_ex);

    Ok((present, swapchain_present, present_ex, reset, reset_ex))
}
