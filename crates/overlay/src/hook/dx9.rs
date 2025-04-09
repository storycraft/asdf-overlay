use core::{ffi::c_void, mem, ptr};

use anyhow::Context;
use parking_lot::RwLock;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Direct3D9::{
            D3D_SDK_VERSION, D3DADAPTER_DEFAULT, D3DCREATE_SOFTWARE_VERTEXPROCESSING,
            D3DDEVTYPE_NULLREF, D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_DISCARD, Direct3DCreate9,
        },
    },
    core::{BOOL, HRESULT, Interface},
};

use super::DetourHook;

type EndSceneFn = unsafe extern "system" fn(*mut c_void) -> HRESULT;

static HOOK: RwLock<Option<DetourHook>> = RwLock::new(None);

unsafe extern "system" fn hooked_end_scene(this: *mut c_void) -> HRESULT {
    let Some(ref end_scene) = *HOOK.read() else {
        return HRESULT(0);
    };

    dbg!("hello dx9 world!");

    unsafe { mem::transmute::<*const (), EndSceneFn>(end_scene.original_fn())(this) }
}

pub fn hook(dummy_hwnd: HWND) -> anyhow::Result<()> {
    let end_scene = get_end_scene_addr(dummy_hwnd)?;

    let present_hook = unsafe { DetourHook::attach(end_scene as _, hooked_end_scene as _)? };
    *HOOK.write() = Some(present_hook);

    Ok(())
}

pub fn cleanup_hook() -> anyhow::Result<()> {
    HOOK.write().take();

    Ok(())
}

/// Get pointer to IDirect3DDevice9::EndScene by creating dummy device
fn get_end_scene_addr(dummy_hwnd: HWND) -> anyhow::Result<EndSceneFn> {
    let device = unsafe {
        let dx9 = Direct3DCreate9(D3D_SDK_VERSION).context("Cannot create IDirect3D9")?;

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

        device.context("Cannot create IDirect3DDevice9")?
    };

    Ok(Interface::vtable(&device).EndScene)
}
