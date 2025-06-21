pub mod data;

use anyhow::bail;
use core::ffi::c_void;
use gl::types::{GLint, GLuint};
use scopeguard::defer;
use tracing::trace;
use windows::{
    Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Texture2D},
    core::Interface,
};

use crate::wgl;

static VERTEX_SHADER: &str = include_str!("opengl/shaders/texture.vert");
static FRAGMENT_SHADER: &str = include_str!("opengl/shaders/texture.frag");

pub struct OpenglRenderer {
    dx_device_handle: *const c_void,

    interop_texture: Option<InteropTexture>,
    texture: GLuint,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
            let dx_device_handle = wgl::DXOpenDeviceNV(device.as_raw());
            if dx_device_handle.is_null() {
                bail!("DXOpenDeviceNV failed");
            }

            let mut texture = 0;
            gl::GenTextures(1, &mut texture);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);

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
                dx_device_handle,

                interop_texture: None,

                texture,
                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn update_texture(&mut self, texture: Option<&ID3D11Texture2D>) -> anyhow::Result<()> {
        let Some(texture) = texture else {
            self.interop_texture.take();
            return Ok(());
        };

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        let size = (desc.Width, desc.Height);
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }

        self.interop_texture = Some(InteropTexture::open_from(
            self.dx_device_handle,
            texture,
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

        let Some(ref interop_texture) = self.interop_texture else {
            return Ok(());
        };

        let rect: [f32; 4] = [
            (position.0 as f32 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 as f32 / screen.1 as f32) * 2.0 + 1.0,
            (size.0 as f32 / screen.0 as f32) * 2.0,
            -(size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        let dx_device_handle = self.dx_device_handle;
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

            wgl::DXLockObjectsNV(
                dx_device_handle,
                1,
                &interop_texture.dx11_tex_handle as *const _ as _,
            );
            defer!({
                wgl::DXUnlockObjectsNV(
                    dx_device_handle,
                    1,
                    &interop_texture.dx11_tex_handle as *const _ as _,
                );
            });
            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        Ok(())
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        self.interop_texture.take();
        unsafe {
            wgl::DXCloseDeviceNV(self.dx_device_handle as _);

            gl::DeleteTextures(1, &self.texture);
            gl::DeleteProgram(self.program);
        }
        trace!("OpenGL resources freed");
    }
}

unsafe impl Send for OpenglRenderer {}
unsafe impl Sync for OpenglRenderer {}

struct InteropTexture {
    owned_device_handle: *const c_void,
    dx11_tex_handle: *const c_void,
}

impl InteropTexture {
    fn open_from(
        dx_device_handle: *const c_void,
        texture: &ID3D11Texture2D,
        gl_texture: GLuint,
    ) -> anyhow::Result<Self> {
        unsafe {
            let dx11_tex_handle = wgl::DXRegisterObjectNV(
                dx_device_handle,
                texture.as_raw(),
                gl_texture,
                gl::TEXTURE_2D,
                wgl::ACCESS_READ_ONLY_NV,
            );
            if dx11_tex_handle.is_null() {
                bail!("DXRegisterObjectNV failed");
            }

            Ok(InteropTexture {
                owned_device_handle: dx_device_handle,
                dx11_tex_handle,
            })
        }
    }
}

impl Drop for InteropTexture {
    fn drop(&mut self) {
        unsafe {
            wgl::DXUnregisterObjectNV(self.owned_device_handle, self.dx11_tex_handle);
        }
    }
}

unsafe impl Send for InteropTexture {}
