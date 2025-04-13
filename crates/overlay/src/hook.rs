mod dx;
mod opengl;

pub use dx::util::call_original_execute_command_lists;
use tracing::debug;

use core::{
    error::Error,
    fmt::{self, Display, Formatter},
};
use std::os::raw::c_void;

use anyhow::Context;
use windows::Win32::{Foundation::HWND, System::Threading::GetCurrentThread};

use crate::detours::{
    DetourAttach, DetourDetach, DetourTransactionBegin, DetourTransactionCommit,
    DetourUpdateThread, LONG,
};

#[tracing::instrument]
pub fn install(dummy_hwnd: HWND) -> anyhow::Result<()> {
    dx::hook(dummy_hwnd).context("Direct3D hook initialization failed")?;
    opengl::hook().context("OpenGL hook initialization failed")?;

    Ok(())
}

#[tracing::instrument]
pub fn cleanup() {
    dx::cleanup();
    opengl::cleanup();
}

struct DetourHook {
    func: *mut (),
    detour: *mut (),
}

impl DetourHook {
    #[tracing::instrument]
    pub unsafe fn attach(func: *mut (), detour: *mut ()) -> DetourResult<Self> {
        let mut func = func.cast::<c_void>();

        unsafe {
            wrap_detour_call(|| DetourTransactionBegin())?;
            wrap_detour_call(|| DetourUpdateThread(GetCurrentThread().0))?;
            wrap_detour_call(|| DetourAttach(&mut func, detour.cast::<c_void>()))?;
            wrap_detour_call(|| DetourTransactionCommit())?;
        }
        debug!("hook attached");

        Ok(DetourHook {
            func: func.cast(),
            detour,
        })
    }

    pub fn original_fn(&self) -> *const () {
        self.func
    }
}

impl Drop for DetourHook {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        let mut func = self.func.cast::<c_void>();

        unsafe {
            wrap_detour_call(|| DetourTransactionBegin()).unwrap();
            wrap_detour_call(|| DetourUpdateThread(GetCurrentThread().0)).unwrap();
            wrap_detour_call(|| DetourDetach(&mut func, self.detour.cast::<c_void>())).unwrap();
            wrap_detour_call(|| DetourTransactionCommit()).unwrap();
        }

        debug!("hook detached");
    }
}

unsafe impl Send for DetourHook {}
unsafe impl Sync for DetourHook {}

type DetourResult<T> = Result<T, DetourError>;

#[derive(Debug, Clone, Copy)]
pub struct DetourError(LONG);

impl Display for DetourError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Detour call error: {}", self.0)
    }
}

impl Error for DetourError {}

#[inline]
fn wrap_detour_call(f: impl FnOnce() -> LONG) -> Result<(), DetourError> {
    let code = f();
    if code == 0 {
        Ok(())
    } else {
        Err(DetourError(code))
    }
}
