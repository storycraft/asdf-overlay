#[repr(align(4))]
struct AlignHelper<T> {
    bytes: T,
}

// Compiled with `glslangvalidator -V texture.vert -o texture_vs.spv`
pub const VERTEX_SHADER: &[u8] = &AlignHelper {
    bytes: *include_bytes!("shaders/texture_vs.spv"),
}
.bytes;

// Compiled with `glslangvalidator -V texture.frag -o texture_fs.spv`
pub const FRAGMENT_SHADER: &[u8] = &AlignHelper {
    bytes: *include_bytes!("shaders/texture_fs.spv"),
}
.bytes;
