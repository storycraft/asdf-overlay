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
mod backend;
mod hook;
mod reader;
mod renderer;
mod texture;
mod util;
mod types;

#[cfg(debug_assertions)]
mod dbg;

use app::app;
use asdf_overlay_common::ipc::create_ipc_path;
use std::{process, thread};
use tokio::runtime::Runtime;

#[inline]
fn proc_impl(name: String) -> bool {
    let Ok(rt) = Runtime::new() else {
        return false;
    };

    if thread::Builder::new()
        .name(name.clone())
        .spawn(move || {
            rt.block_on(app(&create_ipc_path(&name, process::id())));
        })
        .is_err()
    {
        return false;
    }

    true
}

dll_syringe::payload_procedure!(
    fn asdf_overlay_connect(name: String) -> bool {
        proc_impl(name)
    }
);
