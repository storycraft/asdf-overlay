//! Hooking library for Windows using Detours.
//!
//! This crate is intended to be used only as `asdf-overlay`'s internal dependency.
//! It provides a safe abstraction over the Detours library for function hooking.

#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    unused,
    clippy::all
)]
mod bindings {
    // Generated using `bindgen gum_wrapper.h --allowlist-function gum_bindings_.* --use-core --no-layout-tests -o src/bindings.rs`
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
    /// Attach a hook to the target function.
    ///
    /// # Safety
    /// func and detour should be valid function pointers with same signature.
    #[tracing::instrument(level = Level::TRACE)]
    pub unsafe fn attach(func: F, detour: F) -> DetourResult<Self> {
        let mut trampoline: UntypedFnPtr = ptr::null_mut();
        let code = unsafe {
            bindings::gum_bindings_interceptor_replace_fast(
                INTERCEPTER.0,
                func.as_ptr() as _,
                detour.as_ptr() as _,
                (&raw mut trampoline).cast(),
                &DEFAULT_OPTIONS,
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
    #[inline(always)]
    pub unsafe fn original_fn(&self) -> F {
        self.trampoline
    }
}

const DEFAULT_OPTIONS: bindings::GumInterceptorOptions = bindings::GumInterceptorOptions {
    scratch_register: 0,
    scenario: bindings::GumInterceptorScenario_GUM_INTERCEPTOR_SCENARIO_ONLINE,
    relocation_policy: bindings::GumRelocationPolicy_GUM_RELOCATION_FORCED,
    write_redirect: None,
    write_redirect_data: ptr::null_mut(),
    redirect_space_hint: 0,
};

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
