use core::{ffi::c_void, mem, ptr};

use anyhow::Context;
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
    renderer::{Renderers, dx9::Dx9Renderer},
};

use super::HOOK;

pub type EndSceneFn = unsafe extern "system" fn(*mut c_void) -> HRESULT;

#[tracing::instrument]
pub unsafe extern "system" fn hooked_end_scene(this: *mut c_void) -> HRESULT {
    let Some(ref end_scene) = HOOK.read().end_scene else {
        return HRESULT(0);
    };

    {
        let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&this).unwrap() };

        let screen = {
            let desc = unsafe {
                let mut desc = D3DSURFACE_DESC::default();
                let surface = device.GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO).unwrap();
                surface.GetDesc(&mut desc).unwrap();

                desc
            };

            (desc.Width, desc.Height)
        };

        Renderers::with(|renderers| {
            let renderer = renderers.dx9.get_or_insert_with(|| {
                Dx9Renderer::new(device).expect("Dx9Renderer creation failed")
            });
            let position = Overlay::with(|overlay| {
                let size = renderer.size();
                overlay.calc_overlay_position((size.0 as _, size.1 as _), screen)
            });
            _ = renderer.draw(device, position, screen);
        });
    }

    unsafe { mem::transmute::<*const (), EndSceneFn>(end_scene.original_fn())(this) }
}

/// Get pointer to IDirect3DDevice9::EndScene by creating dummy device
pub fn get_end_scene_addr(dummy_hwnd: HWND) -> anyhow::Result<EndSceneFn> {
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

    Ok(Interface::vtable(&device).EndScene)
}
