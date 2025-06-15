// Compiled with `fxc /T vs_5_0 /O3 /Fo texture_vs.o texture.hlsl /E vs_main`
pub const VERTEX_SHADER: &[u8] = include_bytes!("shaders/texture_vs.o");

// Compiled with `fxc /T ps_5_0 /O3 /Fo texture_ps.o texture.hlsl /E ps_main`
pub const PIXEL_SHADER: &[u8] = include_bytes!("shaders/texture_ps.o");
