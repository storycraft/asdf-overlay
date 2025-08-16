fn main() {
    #[cfg(target_env = "msvc")]
    static_vcruntime::metabuild();
}
