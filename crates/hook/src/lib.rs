//! Function hooking for Windows using Frida Gum.
//!
//! This crate is intended to be used only as `asdf-overlay`'s internal dependency.
//! Hook installation is unsafe: callers must uphold function-pointer and code-lifetime
//! requirements. Dropping a hook does not undo the replacement.

#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    unused,
    clippy::all
)]
mod bindings {
    // Generated using `bindgen gum_wrapper.h --allowlist-function gum_bindings_.* --use-core -o src/bindings.rs`
    include!("./bindings.rs");
}

use fn_ptr::{FnPtr, UntypedFnPtr};
use scopeguard::defer;
use tracing::{Level, debug};

use core::{fmt::Debug, ptr};
use std::sync::LazyLock;

/// A detour function hook.
#[derive(Debug)]
pub struct DetourHook<F> {
    trampoline: F,
}

impl<F: FnPtr> DetourHook<F> {
    /// Replace calls to the target with the detour and return an original-call trampoline.
    ///
    /// Even valid function pointers can be rejected when the target's machine code
    /// cannot be intercepted, it is already replaced, or platform policy forbids it.
    /// Dropping the returned hook does not detach it.
    ///
    /// # Safety
    /// Both pointers must have the same signature and calling convention. Their code
    /// must remain loaded while the hook is installed, and the detour must uphold
    /// the target's contract.
    #[tracing::instrument(level = Level::TRACE)]
    pub unsafe fn attach(func: F, detour: F) -> DetourResult<Self> {
        let mut trampoline: UntypedFnPtr = ptr::null_mut();
        let code = unsafe {
            bindings::gum_bindings_interceptor_replace_fast(
                INTERCEPTER.0,
                func.as_ptr() as _,
                detour.as_ptr() as _,
                (&raw mut trampoline).cast(),
            )
        };
        match code {
            bindings::GumReplaceReturn_GUM_REPLACE_WRONG_SIGNATURE => {
                return Err(HookError::BadSignature);
            }
            bindings::GumReplaceReturn_GUM_REPLACE_ALREADY_REPLACED => {
                return Err(HookError::AlreadyReplaced);
            }
            bindings::GumReplaceReturn_GUM_REPLACE_POLICY_VIOLATION => {
                return Err(HookError::PolicyViolation);
            }
            bindings::GumReplaceReturn_GUM_REPLACE_WRONG_TYPE => {
                return Err(HookError::WrongType);
            }

            _ => {}
        }

        debug!("hook attached");
        Ok(DetourHook {
            trampoline: unsafe { F::from_ptr(trampoline as _) },
        })
    }

    /// Get the original function pointer.
    ///
    /// # Safety
    /// The returned function pointer is valid only if the attach transaction is finished and the hook is still attached.
    #[inline(always)]
    pub unsafe fn original_fn(&self) -> F {
        self.trampoline
    }
}

/// Run the closure inside a Frida Gum interceptor transaction, returning its result.
///
/// Returning an error still ends the transaction; this batches hooks and does not
/// roll them back. Cleanup runs on unwinding, but cannot run on process abort.
/// Do not call newly installed trampolines until the transaction has finished.
pub fn with_transaction<R>(f: impl FnOnce() -> R) -> R {
    unsafe {
        bindings::gum_bindings_interceptor_begin_transaction(INTERCEPTER.0);
    }
    defer!(unsafe { bindings::gum_bindings_interceptor_end_transaction(INTERCEPTER.0) });

    f()
}

type DetourResult<T> = Result<T, HookError>;

/// Detour error code.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum HookError {
    #[error("Bad interceptor signature")]
    BadSignature,

    #[error("Function already replaced")]
    AlreadyReplaced,

    #[error("Policy violation")]
    PolicyViolation,

    #[error("Wrong type")]
    WrongType,
}

static INTERCEPTER: LazyLock<Intercepter> = LazyLock::new(|| {
    Intercepter(unsafe {
        bindings::gum_bindings_init();
        bindings::gum_bindings_interceptor_obtain()
    })
});

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct Intercepter(*mut bindings::GumInterceptor);

unsafe impl Send for Intercepter {}
unsafe impl Sync for Intercepter {}
