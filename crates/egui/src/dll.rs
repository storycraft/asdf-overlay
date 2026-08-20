use std::{process::Termination, thread};

use tracing::warn;
use windows::{
    Win32::{
        Foundation::HMODULE,
        System::{
            LibraryLoader::{
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
                GetModuleHandleExA,
            },
            SystemServices::DLL_PROCESS_ATTACH,
        },
    },
    core::PCSTR,
};

#[macro_export]
macro_rules! impl_dll {
    ($main:expr) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case, unused_variables)]
        pub unsafe extern "system" fn DllMain(_: *mut (), fdw_reason: u32, _: *mut ()) -> bool {
            let main = $main;
            unsafe { $crate::dll::dll_main(main, fdw_reason) }
        }
    };
}

#[doc(hidden)]
pub unsafe fn dll_main<F, R>(main: F, fdw_reason: u32) -> bool
where
    F: FnOnce() -> R + Send + 'static,
    R: Termination,
{
    if fdw_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    // Prevent dll from unloading
    unsafe {
        _ = GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            PCSTR(dll_main::<F, R> as *const _),
            &mut HMODULE::default(),
        );
    }

    thread::spawn(|| {
        let code = main().report();
        warn!("DLL main thread exited with code: {code:?}");
    });
    true
}
