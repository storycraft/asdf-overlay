extern crate napi_build;

fn main() {
    napi_build::setup();

    #[cfg(target_env = "msvc")]
    static_vcruntime::metabuild();
}
