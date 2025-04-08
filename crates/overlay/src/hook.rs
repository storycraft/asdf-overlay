pub mod dxgi;
pub mod opengl;

use core::{
    error::Error,
    fmt::{self, Display, Formatter},
};
use std::os::raw::c_void;

use windows::Win32::System::Threading::GetCurrentThread;

use crate::detours::{
    DetourAttach, DetourDetach, DetourTransactionBegin, DetourTransactionCommit,
    DetourUpdateThread, LONG,
};

pub struct DetourHook {
    func: *mut (),
    detour: *mut (),
}

impl DetourHook {
    pub unsafe fn attach(func: *mut (), detour: *mut ()) -> DetourResult<Self> {
        let mut func = func.cast::<c_void>();

        unsafe {
            wrap_detour_call(|| DetourTransactionBegin())?;
            wrap_detour_call(|| DetourUpdateThread(GetCurrentThread().0))?;
            wrap_detour_call(|| DetourAttach(&mut func, detour.cast::<c_void>()))?;
            wrap_detour_call(|| DetourTransactionCommit())?;
        }

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
    fn drop(&mut self) {
        let mut func = self.func.cast::<c_void>();

        unsafe {
            wrap_detour_call(|| DetourTransactionBegin()).unwrap();
            wrap_detour_call(|| DetourUpdateThread(GetCurrentThread().0)).unwrap();
            wrap_detour_call(|| DetourDetach(&mut func, self.detour.cast::<c_void>())).unwrap();
            wrap_detour_call(|| DetourTransactionCommit()).unwrap();
        }
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
