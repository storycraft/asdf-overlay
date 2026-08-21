use anyhow::Context;
use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};
use std::env;
use std::fs::File;
use std::path::Path;

/// Create gl, wgl bindings with extensions.
fn create_gl_bindings(out_dir: &str) -> anyhow::Result<()> {
    let mut gl = File::create(Path::new(&out_dir).join("gl_bindings.rs"))
        .context("Unable to generate gl bindings")?;

    Registry::new(
        Api::Gl,
        (3, 0),
        Profile::Core,
        Fallbacks::None,
        [
            "GL_EXT_memory_object",
            "GL_EXT_win32_keyed_mutex",
            "GL_EXT_memory_object_win32",
        ],
    )
    .write_bindings(GlobalGenerator, &mut gl)
    .context("Couldn't write gl bindings")?;

    let mut wgl = File::create(Path::new(&out_dir).join("wgl_bindings.rs"))
        .context("Unable to generate wgl bindings")?;

    Registry::new(
        Api::Wgl,
        (1, 0),
        Profile::Core,
        Fallbacks::None,
        ["WGL_NV_DX_interop", "WGL_NV_DX_interop2"],
    )
    .write_bindings(GlobalGenerator, &mut wgl)
    .context("Couldn't write wgl bindings")?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dest = env::var("OUT_DIR")?;

    create_gl_bindings(&dest)?;

    Ok(())
}
