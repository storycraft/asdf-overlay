use frida_build::download_and_use_devkit;

fn build_gum() -> anyhow::Result<()> {
    if std::env::var("DOCS_RS").is_ok() {
        return Ok(());
    }

    let include_dir = download_and_use_devkit("gum", include_str!("FRIDA_VERSION").trim());

    cc::Build::new()
        .include(include_dir)
        .file("gum_wrapper.c")
        .compile("bindings");

    if cfg!(target_os = "windows") {
        for lib in [
            "dnsapi", "iphlpapi", "psapi", "winmm", "ws2_32", "advapi32", "crypt32", "gdi32",
            "kernel32", "ole32", "secur32", "shell32", "shlwapi", "user32", "setupapi",
        ] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    build_gum()?;

    Ok(())
}
