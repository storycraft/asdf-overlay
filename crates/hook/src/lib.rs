//! Hooking library for Windows using Detours.
//!
//! This crate is intended to be used only as `asdf-overlay`'s internal dependency.
//! It provides a safe abstraction over the Detours library for function hooking.

#[allow(non_camel_case_types, non_snake_case, unused, clippy::all)]
mod detours {
    // Generated using `bindgen detours_wrapper.h --allowlist-function DetourTransaction.* --allowlist-function DetourAttach --override-abi ".*=stdcall" --use-core -o src/pregenerated.rs`
    #[cfg(target_pointer_width = "32")]
    include!("./pregenerated-x86.rs");
    #[cfg(target_pointer_width = "64")]
    include!("./pregenerated-x64.rs");
}

use tracing::debug;

use core::{
    error::Error,
    ffi::c_long,
    fmt::{self, Debug, Display, Formatter},
};

/// A detour function hook.
#[derive(Debug)]
pub struct DetourHook<F> {
    func: F,
}

impl<F: Copy> DetourHook<F> {
    /// Attach a hook to the target function.
    ///
    /// # Safety
    /// func and detour should be valid function pointers with same signature.
    #[tracing::instrument]
    pub unsafe fn attach(mut func: F, mut detour: F) -> DetourResult<Self>
    where
        F: Debug,
    {
        unsafe {
            wrap_detour_call(|| detours::DetourTransactionBegin())?;
            wrap_detour_call(|| {
                use core::ffi::c_void;

                detours::DetourAttach(
                    (&raw mut func).cast(),
                    *(&raw mut detour).cast::<*mut c_void>(),
                )
            })?;
            wrap_detour_call(|| detours::DetourTransactionCommit())?;
        }
        debug!("hook attached");

        Ok(DetourHook { func })
    }

    /// Get the original function pointer.
    #[inline(always)]
    pub fn original_fn(&self) -> F {
        self.func
    }
}

type DetourResult<T> = Result<T, DetourError>;

/// Detour error code.
#[derive(Debug, Clone, Copy)]
pub struct DetourError(c_long);

impl Display for DetourError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Detour call error: {:?}", self.0)
    }
}

impl Error for DetourError {}

/// Wrap a detour call and convert its errors to `DetourError`.
#[inline]
fn wrap_detour_call(f: impl FnOnce() -> c_long) -> Result<(), DetourError> {
    let code = f();
    if code == 0 {
        Ok(())
    } else {
        Err(DetourError(code))
    }
}
