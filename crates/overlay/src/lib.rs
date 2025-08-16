#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
mod gl {
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

#[allow(unsafe_op_in_unsafe_fn, clippy::all)]
mod wgl {
    include!(concat!(env!("OUT_DIR"), "/wgl_bindings.rs"));
}

pub mod backend;
pub mod event_sink;
pub mod renderer;

mod hook;
mod interop;
mod layout;
mod resources;
mod surface;
mod texture;
mod types;
mod util;

use anyhow::{Context, bail};
use once_cell::sync::OnceCell;
use windows::Win32::Foundation::HINSTANCE;

static INSTANCE: OnceCell<usize> = OnceCell::new();

#[inline]
pub(crate) fn instance() -> HINSTANCE {
    HINSTANCE(*INSTANCE.get().unwrap() as _)
}

pub fn initialize(hinstance: usize) -> anyhow::Result<()> {
    if INSTANCE.set(hinstance).is_err() {
        bail!("Already initialized");
    }

    hook::install(HINSTANCE(hinstance as _)).context("hook initialization failed")?;
    Ok(())
}
