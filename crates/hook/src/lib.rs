#[allow(non_camel_case_types, non_snake_case, unused, clippy::all)]
mod detours {
    include!(concat!(env!("OUT_DIR"), "/detours_bindings.rs"));
}

use tracing::debug;

use core::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};
use std::os::raw::c_void;

use crate::detours::{DetourAttach, DetourTransactionBegin, DetourTransactionCommit, LONG};

#[derive(Debug)]
pub struct DetourHook<F> {
    func: F,
}

impl<F: Copy> DetourHook<F> {
    /// # Safety
    /// func and detour should be valid function pointers with same signature
    #[tracing::instrument]
    pub unsafe fn attach(mut func: F, mut detour: F) -> DetourResult<Self>
    where
        F: Debug,
    {
        unsafe {
            wrap_detour_call(|| DetourTransactionBegin())?;
            wrap_detour_call(|| {
                DetourAttach(
                    (&raw mut func).cast(),
                    *(&raw mut detour).cast::<*mut c_void>(),
                )
            })?;
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
        write!(f, "Detour call error: {:?}", self.0)
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
