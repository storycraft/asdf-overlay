use core::{ffi::c_void, ptr};

use anyhow::Context;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Direct3D9::{
            D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DCREATE_HARDWARE_VERTEXPROCESSING,
            D3DDEVICE_CREATION_PARAMETERS, D3DDEVTYPE_HAL, D3DDISPLAYMODEEX, D3DPRESENT_PARAMETERS,
            D3DSWAPEFFECT_DISCARD, Direct3DCreate9Ex, IDirect3DDevice9,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    backend::{Backends, renderers::Renderer},
    reader::SharedHandleReader,
    renderer::dx9::Dx9Renderer,
};

use super::HOOK;

pub type EndSceneFn = unsafe extern "system" fn(*mut c_void) -> HRESULT;
pub type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;
pub type ResetExFn = unsafe extern "system" fn(
    *mut c_void,
    *mut D3DPRESENT_PARAMETERS,
    *mut D3DDISPLAYMODEEX,
) -> HRESULT;

#[tracing::instrument]
pub(super) extern "system" fn hooked_end_scene(this: *mut c_void) -> HRESULT {
    trace!("EndScene called");

    let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();
    let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();

    let mut params = D3DPRESENT_PARAMETERS::default();
    unsafe { swapchain.GetPresentParameters(&mut params) }.unwrap();
    if !params.hDeviceWindow.is_invalid() {
        draw_overlay(params.hDeviceWindow, device);
    }

    let end_scene = HOOK.end_scene.get().unwrap();
    unsafe { end_scene.original_fn()(this) }
}

#[inline]
fn draw_overlay(hwnd: HWND, device: &IDirect3DDevice9) {
    let res = Backends::with_or_init_backend(hwnd, |backend| {
        let renderer = match backend.renderer {
            Some(Renderer::Dx9(ref mut renderer)) => renderer,
            Some(_) => {
                trace!("ignoring dx9 rendering");
                return;
            }
            None => {
                debug!("Found dx9 window");
                backend.renderer = Some(Renderer::Dx9(None));
                // wait next swap for possible remaining renderer check
                return;
            }
        };
        trace!("using dx9 renderer");
        let renderer = renderer
            .get_or_insert_with(|| Dx9Renderer::new(device).expect("Dx9Renderer creation failed"));

        let reader = backend
            .cx
            .fallback_reader
            .get_or_insert_with(|| SharedHandleReader::new().unwrap());

        if backend.surface.invalidate_update() && backend.surface.get().is_none() {
            renderer.reset_texture();
        }
        let Some(surface) = backend.surface.get() else {
            return;
        };

        let screen = backend.size;
        let size = surface.size();
        let position = backend
            .layout
            .calc_position((size.0 as _, size.1 as _), screen);

        let interop = &mut backend.interop;
        match reader.with_mapped(
            &interop.device,
            surface.mutex(),
            interop.cx.get_mut(),
            surface.texture(),
            size,
            |mapped| renderer.update_texture(device, size, mapped),
        ) {
            Ok(Some(_)) => {
                let _res = renderer.draw(device, position, screen);
                trace!("dx9 render: {:?}", _res);
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
            let Some(Renderer::Dx9(ref mut renderer)) = backend.renderer else {
                return;
            };
            renderer.take();
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

    let reset = HOOK.reset.get().unwrap();
    unsafe { reset.original_fn()(this, param) }
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

    let reset_ex = HOOK.reset_ex.get().unwrap();
    unsafe { reset_ex.original_fn()(this, param, fullscreen_display_mode) }
}

/// Get pointer to IDirect3DDevice9::EndScene, IDirect3DDevice9::Reset by creating dummy device
pub fn get_dx9_addr(dummy_hwnd: HWND) -> anyhow::Result<(EndSceneFn, ResetFn, ResetExFn)> {
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

    let dx9_vtable = Interface::vtable(&*device);
    let end_scene = dx9_vtable.EndScene;
    debug!("IDirect3DDevice9::EndScene found: {:p}", end_scene);

    let reset = dx9_vtable.Reset;
    debug!("IDirect3DDevice9::Reset found: {:p}", reset);

    let dx9ex_vtable = Interface::vtable(&device);
    let reset_ex = dx9ex_vtable.ResetEx;
    debug!("IDirect3DDevice9Ex::ResetEx found: {:p}", reset_ex);

    Ok((end_scene, reset, reset_ex))
}
