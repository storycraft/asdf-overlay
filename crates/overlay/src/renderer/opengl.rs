pub mod data;

use core::ffi::c_void;

use crate::gl::{
    self,
    types::{GLint, GLuint},
};
use tracing::trace;
use windows::{
    Win32::Graphics::{
        Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D},
        Dxgi::IDXGIResource,
    },
    core::Interface,
};

static VERTEX_SHADER: &str = include_str!("opengl/shaders/texture.vert");
static FRAGMENT_SHADER: &str = include_str!("opengl/shaders/texture.frag");

pub struct OpenglRenderer {
    interop: Option<GlInterop>,
    texture: GLuint,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
            let mut texture = 0;
            gl::GenTextures(1, &mut texture);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_TILING_EXT,
                gl::OPTIMAL_TILING_EXT as _,
            );

            let vert_shader = gl::CreateShader(gl::VERTEX_SHADER);
            gl::ShaderSource(
                vert_shader,
                1,
                (&raw const VERTEX_SHADER).cast(),
                &(VERTEX_SHADER.len() as i32),
            );
            gl::CompileShader(vert_shader);

            let frag_shader = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(
                frag_shader,
                1,
                (&raw const FRAGMENT_SHADER).cast(),
                &(FRAGMENT_SHADER.len() as i32),
            );
            gl::CompileShader(frag_shader);

            let program = gl::CreateProgram();
            gl::AttachShader(program, vert_shader);
            gl::AttachShader(program, frag_shader);
            gl::LinkProgram(program);

            let rect_loc = gl::GetUniformLocation(program, b"rect\0" as *const _ as _);
            let tex_loc = gl::GetUniformLocation(program, b"tex\0" as *const _ as _);

            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            Ok(Self {
                interop: None,

                texture,
                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn update_texture(&mut self, texture: Option<&ID3D11Texture2D>) -> anyhow::Result<()> {
        // drop previous texture first, so it can be opened again without error
        self.interop.take();
        let Some(texture) = texture else {
            return Ok(());
        };

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        let size = (desc.Width, desc.Height);
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }

        self.interop = Some(GlInterop::open_to(
            unsafe { texture.cast::<IDXGIResource>()?.GetSharedHandle()? }
                .0
                .cast(),
            (size.0 as _, size.1 as _),
            self.texture,
        )?);
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        position: (i32, i32),
        size: (u32, u32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        if self.interop.is_none() {
            return Ok(());
        };

        let rect: [f32; 4] = [
            (position.0 as f32 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 as f32 / screen.1 as f32) * 2.0 + 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            -(size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            gl::Viewport(0, 0, screen.0 as _, screen.1 as _);

            gl::Enable(gl::BLEND);
            gl::BlendEquation(gl::FUNC_ADD);
            gl::BlendFuncSeparate(
                gl::SRC_ALPHA,
                gl::ONE_MINUS_SRC_ALPHA,
                gl::ONE,
                gl::ONE_MINUS_SRC_ALPHA,
            );
            gl::Disable(gl::CULL_FACE);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);

            gl::UseProgram(self.program);
            gl::Uniform4f(self.rect_loc, rect[0], rect[1], rect[2], rect[3]);
            gl::Uniform1i(self.tex_loc, 0);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        Ok(())
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        self.interop.take();
        unsafe {
            // wgl::DXCloseDeviceNV(self.dx_device_handle as _);

            gl::DeleteTextures(1, &self.texture);
            gl::DeleteProgram(self.program);
        }
        trace!("OpenGL resources freed");
    }
}

unsafe impl Send for OpenglRenderer {}
unsafe impl Sync for OpenglRenderer {}

struct GlInterop {
    memory_object: GLuint,
}

impl GlInterop {
    fn open_to(handle: *mut c_void, size: (i32, i32), gl_texture: GLuint) -> anyhow::Result<Self> {
        unsafe {
            let mut memory_object = 0;
            gl::CreateMemoryObjectsEXT(1, &mut memory_object);
            gl::MemoryObjectParameterivEXT(memory_object, gl::DEDICATED_MEMORY_OBJECT_EXT, &1);

            gl::ImportMemoryWin32HandleEXT(
                memory_object,
                0,
                gl::HANDLE_TYPE_D3D11_IMAGE_KMT_EXT,
                handle,
            );

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, gl_texture);
            gl::TexStorageMem2DEXT(
                gl::TEXTURE_2D,
                1,
                gl::BGRA,
                size.0 as _,
                size.1 as _,
                memory_object,
                0,
            );

            Ok(Self { memory_object })
        }
    }
}

impl Drop for GlInterop {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteMemoryObjectsEXT(1, &self.memory_object);
        }
    }
}

unsafe impl Send for GlInterop {}
