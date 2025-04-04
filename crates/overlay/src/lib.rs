#![windows_subsystem = "windows"]

#[allow(unsafe_op_in_unsafe_fn)]
mod wgl {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

mod app;
mod hook;
mod renderer;

use core::ffi::c_void;
use std::thread;

use app::main;
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
            if let Err(err) = rt.block_on(main()) {
                eprintln!("dll error: {err}");
            }
            drop(rt);

            unsafe {
                FreeLibraryAndExitThread(HMODULE(dll_module.0), 0);
            }
        }
    });

    Ok(())
}

#[unsafe(no_mangle)]
#[allow(non_snake_case, unused_variables)]
pub extern "system" fn DllMain(dll_module: HINSTANCE, reason: u32, reserved: *mut c_void) -> BOOL {
    if let Err(err) = match reason {
        DLL_PROCESS_ATTACH => attach(dll_module),
        _ => Ok(()),
    } {
        eprintln!("dll attach failed. {err}");
    }

    BOOL(1)
}
