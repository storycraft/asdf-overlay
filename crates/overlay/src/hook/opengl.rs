mod cx;

use core::{ffi::c_void, mem};
use std::ffi::CString;

use anyhow::Context;
use cx::OverlayGlContext;
use parking_lot::Mutex;
use retour::GenericDetour;
use windows::{
    core::{BOOL, PCSTR}, Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Gdi::{WindowFromDC, HDC},
            OpenGL::wglGetProcAddress,
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
        UI::WindowsAndMessaging::GetClientRect,
    }
};

use crate::{renderer::opengl::OpenglRenderer, wgl};

pub fn hook() -> anyhow::Result<()> {
    let original = get_opengl_wglswapbuffers_addr()?;
    let hook = unsafe { Hook::new(original, hooked)? };
    unsafe { hook.enable()? };
    *HOOK.lock() = Some(hook);

    Ok(())
}

pub fn cleanup_hook() -> anyhow::Result<()> {
    let Some(hook) = HOOK.lock().take() else {
        return Ok(());
    };

    unsafe { hook.disable()? };

    RENDERER.lock().take();

    Ok(())
}

type WglSwapBuffersFn = unsafe extern "system" fn(*mut c_void) -> BOOL;

fn get_opengl_wglswapbuffers_addr() -> anyhow::Result<WglSwapBuffersFn> {
    // Grab a handle to opengl32.dll
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    let wglswapbuffers = CString::new("wglSwapBuffers")?;
    let func = unsafe {
        GetProcAddress(opengl32module, PCSTR(wglswapbuffers.as_ptr() as *mut _))
            .context("wglSwapBuffers not found")?
    };

    Ok(unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, WglSwapBuffersFn>(func) })
}

type Hook = GenericDetour<WglSwapBuffersFn>;

static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

pub static RENDERER: Mutex<Option<OpenglRenderer>> = Mutex::new(None);
static CX: Mutex<Option<OverlayGlContext>> = Mutex::new(None);

unsafe extern "system" fn hooked(hdc: *mut c_void) -> BOOL {
    let Some(ref mut hook) = *HOOK.lock() else {
        return BOOL(0);
    };

    let mut cx = CX.lock();
    let cx = cx.get_or_insert_with(|| OverlayGlContext::new(HDC(hdc)).unwrap());

    cx.with(HDC(hdc), || {
        let mut renderer = RENDERER.lock();
        let renderer = renderer.get_or_insert_with(|| {
            setup_gl().unwrap();

            OpenglRenderer::new()
        });

        let mut rect = RECT::default();
        unsafe { GetClientRect(WindowFromDC(HDC(hdc)), &mut rect).unwrap() };

        renderer.draw(((rect.right - rect.left) as _, (rect.bottom - rect.top) as _));
    });

    unsafe { hook.call(hdc) }
}

fn setup_gl() -> anyhow::Result<()> {
    let opengl32dll = CString::new("opengl32.dll")?;
    let opengl32module = unsafe { GetModuleHandleA(PCSTR(opengl32dll.as_ptr() as *mut _))? };

    fn loader(module: HMODULE, s: &str) -> *const c_void {
        let name = CString::new(s).unwrap();

        unsafe {
            let addr = PCSTR(name.as_ptr() as _);
            let fn_ptr = wglGetProcAddress(addr);
            if let Some(ptr) = fn_ptr {
                ptr as _
            } else {
                GetProcAddress(module, addr)
                    .map_or(std::ptr::null(), |fn_ptr| fn_ptr as *const _)
            }
        }
    }

    wgl::load_with(|s| loader(opengl32module, s));
    gl::load_with(|s| loader(opengl32module, s));

    Ok(())
}
