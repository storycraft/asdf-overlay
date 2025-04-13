#![windows_subsystem = "windows"]

#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
mod wgl {
    include!(concat!(env!("OUT_DIR"), "/wgl_bindings.rs"));
}

#[allow(non_camel_case_types, non_snake_case, unused, clippy::all)]
mod detours {
    include!(concat!(env!("OUT_DIR"), "/detours_bindings.rs"));
}

mod app;
mod hook;
mod renderer;
mod util;

use app::main;
use core::ffi::c_void;
use std::thread;
use tokio::runtime::Runtime;
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HMODULE},
        System::{LibraryLoader::FreeLibraryAndExitThread, SystemServices::DLL_PROCESS_ATTACH},
    },
    core::BOOL,
};

fn attach(dll_module: HINSTANCE) -> anyhow::Result<()> {
    let rt = Runtime::new()?;

    thread::spawn({
        struct Wrapper(*mut c_void);
        unsafe impl Send for Wrapper {}

        let dll_module = Wrapper(dll_module.0);

        move || {
            let dll_module = dll_module;
            rt.block_on(main());
            drop(rt);

            unsafe {
                FreeLibraryAndExitThread(HMODULE(dll_module.0), 0);
            }
        }
    });

    Ok(())
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(dll_module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if let Err(err) = match reason {
        DLL_PROCESS_ATTACH => attach(dll_module),
        _ => Ok(()),
    } {
        #[cfg(debug_assertions)]
        eprintln!("dll initialization failed. {err}");
    }

    BOOL(1)
}
