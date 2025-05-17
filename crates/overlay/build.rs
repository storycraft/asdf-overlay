use anyhow::{Context, bail};
use cc::windows_registry::find_tool;
use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};
use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};
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

fn create_detours_bindings(out_dir: &str) -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=detours_wrapper.h");

    let dir = env::var("CARGO_MANIFEST_DIR")?;

    let tool = find_tool("x86_64-pc-windows-msvc", "msbuild").context("msbuild not found")?;

    let platform = match env::var("TARGET")?.as_str() {
        "x86_64-pc-windows-msvc" => "x64",
        "i686-pc-windows-msvc" => "x86",
        "aarch64-pc-windows-msvc" => "ARM64",
        target => bail!("Unsupported target {}", target),
    };

    if !tool
        .to_command()
        .args([
            "detours\\vc\\Detours.sln",
            "/p:Configuration=ReleaseMD",
            &format!("/p:Platform={}", platform),
        ])
        .output()?
        .status
        .success()
    {
        bail!("Detour build failed");
    }

    // Tell cargo to look for shared libraries in the specified directory
    println!(
        "cargo:rustc-link-search={}",
        PathBuf::from(&dir)
            .join("detours")
            .join(format!("lib.{}", platform))
            .display()
    );

    // link detours
    println!("cargo:rustc-link-lib=detours");
    println!("cargo:rustc-link-lib=syelog");

    let bindings = bindgen::Builder::default()
        .header(camino::Utf8PathBuf::from(dir).join("detours_wrapper.h"))
        .allowlist_function("Detour.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .context("Unable to generate detours bindings")?;

    bindings
        .write_to_file(Path::new(out_dir).join("detours_bindings.rs"))
        .context("Couldn't write detours bindings")?;

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
    create_detours_bindings(&dest)?;
    create_rc()?;

    Ok(())
}
