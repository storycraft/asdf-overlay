use anyhow::Context;
use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};
use std::env;
use std::fs::File;
use std::path::Path;
use winres::WindowsResource;

fn create_gl_bindings(out_dir: &str) -> anyhow::Result<()> {
    let mut file = File::create(Path::new(&out_dir).join("wgl_bindings.rs"))
        .context("Unable to generate wgl bindings")?;

    Registry::new(
        Api::Wgl,
        (1, 0),
        Profile::Core,
        Fallbacks::None,
        ["WGL_NV_DX_interop", "WGL_NV_DX_interop2"],
    )
    .write_bindings(GlobalGenerator, &mut file)
    .context("Couldn't write wgl bindings")?;

    Ok(())
}

fn create_rc() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=resources");
    let mut res = WindowsResource::new();
    res.append_rc_content(include_str!("./resources/cursors.rc"));
    res.compile()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dest = env::var("OUT_DIR")?;

    create_gl_bindings(&dest)?;
    create_rc()?;

    Ok(())
}
