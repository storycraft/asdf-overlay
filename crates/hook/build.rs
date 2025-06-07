use anyhow::Context;
use std::env;
use std::path::Path;

fn create_detours_bindings(out_dir: &str) -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=detours_wrapper.h");

    cc::Build::new()
        .define(
            "DETOUR_DEBUG",
            if env::var("DEBUG").unwrap() == "true" {
                "1"
            } else {
                "0"
            },
        )
        .file("detours/src/detours.cpp")
        .file("detours/src/modules.cpp")
        .file("detours/src/disasm.cpp")
        .file("detours/src/image.cpp")
        .file("detours/src/creatwth.cpp")
        .file("detours/src/disasm.cpp")
        .compile("detours");

    let bindings = bindgen::Builder::default()
        .header("detours_wrapper.h")
        .allowlist_function("Detour.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .context("Unable to generate detours bindings")?;

    bindings
        .write_to_file(Path::new(out_dir).join("detours_bindings.rs"))
        .context("Couldn't write detours bindings")?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dest = env::var("OUT_DIR")?;

    create_detours_bindings(&dest)?;

    Ok(())
}
