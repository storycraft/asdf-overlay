use std::env;

fn build_detours() -> anyhow::Result<()> {
    if std::env::var("DOCS_RS").is_ok() {
        return Ok(());
    }

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

    Ok(())
}

fn main() -> anyhow::Result<()> {
    build_detours()?;

    Ok(())
}
