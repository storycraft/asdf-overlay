mod dx;
mod opengl;

pub use dx::util::call_original_execute_command_lists;

use tracing::debug;

use core::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};
use std::os::raw::c_void;

use anyhow::Context;
use windows::Win32::Foundation::HWND;

use crate::detours::{DetourAttach, DetourTransactionBegin, DetourTransactionCommit, LONG};

#[tracing::instrument]
pub fn install(dummy_hwnd: HWND) -> anyhow::Result<()> {
    dx::hook(dummy_hwnd).context("Direct3D hook initialization failed")?;
    opengl::hook().context("OpenGL hook initialization failed")?;

    Ok(())
}

#[tracing::instrument]
pub fn cleanup() {
    dx::cleanup();
}

struct DetourHook<F> {
    func: F,
}

impl<F: Copy> DetourHook<F> {
    #[tracing::instrument]
    pub unsafe fn attach(mut func: F, detour: *mut ()) -> DetourResult<Self>
    where
        F: Debug,
    {
        unsafe {
            wrap_detour_call(|| DetourTransactionBegin())?;
            wrap_detour_call(|| DetourAttach((&raw mut func).cast(), detour.cast::<c_void>()))?;
            wrap_detour_call(|| DetourTransactionCommit())?;
        }
        debug!("hook attached");

        Ok(DetourHook { func })
    }

    #[inline]
    pub fn original_fn(&self) -> F {
        self.func
    }
}

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
