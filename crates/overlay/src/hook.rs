mod dx;
mod opengl;

use dashmap::DashMap;
pub use dx::util::call_original_execute_command_lists;
use once_cell::sync::Lazy;
use rustc_hash::FxBuildHasher;
use tracing::{debug, trace};

use core::{
    error::Error,
    fmt::{self, Display, Formatter},
};
use std::os::raw::c_void;

use anyhow::Context;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND},
    System::Threading::{GetCurrentThread, GetCurrentThreadId},
};

use crate::detours::{
    DetourAttach, DetourDetach, DetourTransactionBegin, DetourTransactionCommit,
    DetourUpdateThread, LONG,
};

struct ThreadHandle(HANDLE);

impl Drop for ThreadHandle {
    fn drop(&mut self) {
        _ = unsafe { CloseHandle(self.0) };
    }
}

unsafe impl Send for ThreadHandle {}
unsafe impl Sync for ThreadHandle {}

static HOOK_THREADS: Lazy<DashMap<u32, ThreadHandle, FxBuildHasher>> =
    Lazy::new(|| DashMap::with_hasher(FxBuildHasher::default()));

fn collect_hook_thread() {
    let id = unsafe { GetCurrentThreadId() };
    if !HOOK_THREADS.contains_key(&id) {
        debug!("collecting thread {} accessing hooks", id);
        let handle = ThreadHandle(unsafe { GetCurrentThread() });
        HOOK_THREADS.insert(id, handle);
    }
}

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

    HOOK_THREADS.clear();
    HOOK_THREADS.shrink_to_fit();
    debug!("hook removed");
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
    fn drop(&mut self) {
        let mut func = self.func.cast::<c_void>();

        unsafe {
            wrap_detour_call(|| DetourTransactionBegin()).unwrap();
            for item in HOOK_THREADS.iter() {
                if wrap_detour_call(|| DetourUpdateThread(item.0.0)).is_ok() {
                    trace!("suspending thread {} while detaching hook", item.key());
                }
            }
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
