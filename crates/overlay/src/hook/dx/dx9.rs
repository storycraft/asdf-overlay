use core::{ffi::c_void, mem, ptr};

use anyhow::Context;
use parking_lot::Mutex;
use tracing::{debug, trace};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Direct3D9::{
            D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DBACKBUFFER_TYPE_MONO,
            D3DCREATE_SOFTWARE_VERTEXPROCESSING, D3DDEVTYPE_NULLREF, D3DPRESENT_PARAMETERS,
            D3DSURFACE_DESC, D3DSWAPEFFECT_DISCARD, Direct3DCreate9, IDirect3DDevice9,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use crate::{
    app::Overlay,
    reader::SharedHandleReader,
    renderer::{Renderers, dx9::Dx9Renderer},
};

use super::HOOK;

pub type EndSceneFn = unsafe extern "system" fn(*mut c_void) -> HRESULT;
pub type ResetFn = unsafe extern "system" fn(*mut c_void, *mut D3DPRESENT_PARAMETERS) -> HRESULT;

static READER: Mutex<Option<SharedHandleReader>> = Mutex::new(None);

pub fn cleanup() {
    READER.lock().take();
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_end_scene(this: *mut c_void) -> HRESULT {
    let Some(ref end_scene) = HOOK.read().end_scene else {
        return HRESULT(0);
    };
    trace!("EndScene called");

    let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this) }.unwrap();

    let screen = {
        let mut desc = D3DSURFACE_DESC::default();
        unsafe {
            let surface = device.GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO).unwrap();
            surface.GetDesc(&mut desc).unwrap();
        }

        (desc.Width, desc.Height)
    };

    let mut reader = READER.lock();
    let reader = reader.get_or_insert_with(|| SharedHandleReader::new().unwrap());

    Renderers::with(|renderers| {
        let renderer = renderers
            .dx9
            .get_or_insert_with(|| Dx9Renderer::new(device).expect("Dx9Renderer creation failed"));
        let position = Overlay::with(|overlay| {
            let size = renderer.size();

            if let Some(shared) = overlay.take_pending_handle() {
                reader.update_shared(shared);
            }

            overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
        });

        _ = reader.with_mapped(|size, mapped| {
            renderer.update_texture(device, size, mapped)?;

            Ok(())
        });

        _ = renderer.draw(device, position, screen);
    });

    unsafe { mem::transmute::<*const (), EndSceneFn>(end_scene.original_fn())(this) }
}

#[tracing::instrument]
pub unsafe extern "system" fn hooked_reset(
    this: *mut c_void,
    param: *mut D3DPRESENT_PARAMETERS,
) -> HRESULT {
    let Some(ref reset) = HOOK.read().reset else {
        return HRESULT(0);
    };

    Renderers::with(|renderers| {
        renderers.dx9.take();
    });

    unsafe { mem::transmute::<*const (), ResetFn>(reset.original_fn())(this, param) }
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
