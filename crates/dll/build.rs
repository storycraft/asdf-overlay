use core::error::Error;
use std::io;
use winres::WindowsResource;

/// Create Windows cursor resources.
fn create_rc() -> io::Result<()> {
    println!("cargo:rerun-if-changed=resources");
    let mut res = WindowsResource::new();
    res.append_rc_content(include_str!("./resources/cursors.rc"));
    res.compile()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(target_env = "msvc")]
    static_vcruntime::metabuild();

    if std::env::var("DOCS_RS").is_ok() {
        return Ok(());
    }

    create_rc()?;

    Ok(())
}
