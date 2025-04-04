use gl_generator::{Api, Fallbacks, GlobalGenerator, Profile, Registry};
use std::env;
use std::fs::File;
use std::path::Path;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("bindings.rs")).unwrap();

    Registry::new(Api::Wgl, (1, 0), Profile::Core, Fallbacks::None, [
        "WGL_NV_DX_interop",
        "WGL_NV_DX_interop2"
    ])
        .write_bindings(GlobalGenerator, &mut file)
        .unwrap();
}
