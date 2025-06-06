use anyhow::{Context, bail};
use cc::windows_registry::find_tool;
use file_guard::Lock;
use std::env;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

fn create_detours_bindings(out_dir: &str) -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=detours_wrapper.h");

    let mut lockfile = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(".detours-lock")?;
    let _lock = file_guard::lock(&mut lockfile, Lock::Exclusive, 0, 1)?;

    let dir = env::var("CARGO_MANIFEST_DIR")?;

    let target = env::var("TARGET")?;
    let target = target.as_str();
    let platform = match target {
        "x86_64-pc-windows-msvc" => "x64",
        "i686-pc-windows-msvc" => "x86",
        "aarch64-pc-windows-msvc" => "ARM64",
        target => bail!("Unsupported target {}", target),
    };

    let tool = find_tool("x86_64-pc-windows-msvc", "msbuild").context("msbuild not found")?;
    let output = tool
        .to_command()
        .args([
            "detours\\vc\\Detours.sln",
            "/p:Configuration=ReleaseMD",
            &format!("/p:Platform={platform}"),
        ])
        .output()?;
    if !output.status.success() {
        eprintln!(
            "error: {}",
            str::from_utf8(&output.stdout).ok().unwrap_or_default()
        );
        bail!("Detour build failed");
    }

    // Tell cargo to look for shared libraries in the specified directory
    println!(
        "cargo:rustc-link-search={}",
        PathBuf::from(&dir)
            .join("detours")
            .join(format!("lib.{platform}"))
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

fn main() -> anyhow::Result<()> {
    let dest = env::var("OUT_DIR")?;

    create_detours_bindings(&dest)?;

    Ok(())
}
