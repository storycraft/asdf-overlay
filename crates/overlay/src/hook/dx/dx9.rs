use core::{ffi::c_void, ptr};

use anyhow::Context;
use tracing::{debug, error, trace};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Direct3D9::{
            D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DCREATE_SOFTWARE_VERTEXPROCESSING,
            D3DDEVTYPE_NULLREF, D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_DISCARD, Direct3DCreate9,
            IDirect3DDevice9,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    app::Overlay, backend::Backends, reader::SharedHandleReader, renderer::dx9::Dx9Renderer,
};

use super::HOOK;

pub type EndSceneFn = unsafe extern "system" fn(*mut c_void) -> HRESULT;
pub type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;

#[tracing::instrument]
pub unsafe extern "system" fn hooked_end_scene(this: *mut c_void) -> HRESULT {
    trace!("EndScene called");

    _ = Overlay::with(|overlay| {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();

        let swapchain = unsafe { device.GetSwapChain(0) }.unwrap();

        let mut params = D3DPRESENT_PARAMETERS::default();
        unsafe { swapchain.GetPresentParameters(&mut params) }.unwrap();

        if params.hDeviceWindow.is_invalid() {
            error!("invalid hDeviceWindow");
            return;
        }

        let res = Backends::with_or_init_backend(params.hDeviceWindow, |backend| {
            let reader = backend
                .cx
                .fallback_reader
                .get_or_insert_with(|| SharedHandleReader::new().unwrap());
            let screen = backend.size;

            trace!("using dx9 renderer");
            let renderer = backend.renderer.dx9.get_or_insert_with(|| {
                Dx9Renderer::new(device).expect("Dx9Renderer creation failed")
            });

            if let Some(shared) = backend.pending_handle.take() {
                reader.update_shared(shared);
            }

            let size = renderer.size();
            let position = overlay.calc_overlay_position((size.0 as _, size.1 as _), screen);

            _ = reader.with_mapped(|size, mapped| {
                renderer.update_texture(device, size, mapped)?;

                Ok(())
            });

            _ = renderer.draw(device, position, screen);
        });

        if let Err(_err) = res {
            error!("Backends::with_or_init_backend failed. err: {:?}", _err);
        }
    });

    let end_scene = HOOK.end_scene.get().unwrap();
    unsafe { end_scene.original_fn()(this) }
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_reset(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
) -> HRESULT {
    let hwnd = unsafe { &*param }.hDeviceWindow;
    if !hwnd.is_invalid() {
        Backends::with_backend(hwnd, |backend| {
            backend.renderer.dx9.take();
        })
        .expect("Backends::with_backend failed");
    }

    let reset = HOOK.reset.get().unwrap();
    unsafe { reset.original_fn()(this, param) }
}

/// Get pointer to IDirect3DDevice9::EndScene, IDirect3DDevice9::Reset by creating dummy device
pub fn get_dx9_addr(dummy_hwnd: HWND) -> anyhow::Result<(EndSceneFn, ResetFn)> {
    let device = unsafe {
        let dx9 = Direct3DCreate9(D3D_SDK_VERSION).context("cannot create IDirect3D9")?;

        let mut device = None;
        dx9.CreateDevice(
            D3DADAPTER_DEFAULT,
            D3DDEVTYPE_NULLREF,
            HWND(ptr::null_mut()),
            D3DCREATE_SOFTWARE_VERTEXPROCESSING as _,
            &mut D3DPRESENT_PARAMETERS {
                Windowed: BOOL(1),
                SwapEffect: D3DSWAPEFFECT_DISCARD,
                hDeviceWindow: dummy_hwnd,
                ..Default::default()
            },
            &mut device,
        )?;

        device.context("cannot create IDirect3DDevice9")?
    };
    let end_scene = Interface::vtable(&device).EndScene;
    debug!("IDirect3DDevice9::EndScene found: {:p}", end_scene);

    let reset = Interface::vtable(&device).Reset;
    debug!("IDirect3DDevice9::Reset found: {:p}", reset);

    Ok((end_scene, reset))
}
