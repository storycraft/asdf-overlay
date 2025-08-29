pub mod data;

use core::ffi::c_void;

use crate::{
    gl::{
        self,
        types::{GLint, GLuint},
    },
    wgl,
};
use anyhow::bail;
use scopeguard::defer;
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
    interop: Option<GlInteropTexture>,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
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

                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn update_texture(&mut self, texture: Option<&ID3D11Texture2D>) -> anyhow::Result<()> {
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

        self.interop = Some(GlInteropTexture::new(texture, (size.0 as _, size.1 as _))?);
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

        let Some(ref texture) = self.interop else {
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
            texture.bind(gl::TEXTURE_2D, || {
                gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
            });
        }

        Ok(())
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        self.interop.take();
        unsafe {
            gl::DeleteProgram(self.program);
        }
        trace!("OpenGL resources freed");
    }
}

unsafe impl Send for OpenglRenderer {}
unsafe impl Sync for OpenglRenderer {}

enum GlInteropTexture {
    MemoryObject(MemoryObjectTexture),
    Wgl(NvInteropTexture),
}

impl GlInteropTexture {
    pub fn new(texture: &ID3D11Texture2D, size: (u32, u32)) -> anyhow::Result<Self> {
        if gl::ImportMemoryWin32HandleEXT::is_loaded()
            && let Ok(memory_object) = MemoryObjectTexture::open(texture, size)
        {
            Ok(Self::MemoryObject(memory_object))
        } else if wgl::DXOpenDeviceNV::is_loaded() {
            let device = unsafe { texture.GetDevice()? };
            Ok(Self::Wgl(NvInteropTexture::open(&device, texture)?))
        } else {
            bail!("Opengl interop is not supported");
        }
    }

    #[inline]
    pub fn bind(&self, target: gl::types::GLenum, f: impl FnOnce()) {
        match *self {
            Self::MemoryObject(ref texture) => texture.bind(target, f),
            Self::Wgl(ref texture) => texture.bind(target, f),
        }
    }
}

struct MemoryObjectTexture {
    memory_object: GLuint,
    id: GLuint,
}

impl MemoryObjectTexture {
    fn open(texture: &ID3D11Texture2D, size: (u32, u32)) -> anyhow::Result<Self> {
        unsafe {
            let handle = texture.cast::<IDXGIResource>()?.GetSharedHandle()?.0.cast();

            let mut memory_object = 0;
            gl::CreateMemoryObjectsEXT(1, &mut memory_object);

            // reset previous error before
            _ = gl::GetError();
            gl::ImportMemoryWin32HandleEXT(
                memory_object,
                0,
                gl::HANDLE_TYPE_D3D11_IMAGE_KMT_EXT,
                handle,
            );
            if gl::GetError() != gl::NO_ERROR {
                gl::DeleteMemoryObjectsEXT(1, &memory_object);
                bail!("ImportMemoryWin32HandleEXT failed");
            }

            let mut texture = 0;
            gl::GenTextures(1, &mut texture);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
            gl::TexStorageMem2DEXT(
                gl::TEXTURE_2D,
                1,
                gl::BGRA,
                size.0 as _,
                size.1 as _,
                memory_object,
                0,
            );

            Ok(Self {
                memory_object,
                id: texture,
            })
        }
    }

    #[inline]
    pub fn bind(&self, target: gl::types::GLenum, f: impl FnOnce()) {
        unsafe { gl::BindTexture(target, self.id) };
        f();
    }
}

impl Drop for MemoryObjectTexture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
            gl::DeleteMemoryObjectsEXT(1, &self.memory_object);
        }
    }
}

unsafe impl Send for MemoryObjectTexture {}

struct NvInteropTexture {
    device_handle: *const c_void,
    dx11_tex_handle: *const c_void,
    gl_texture: GLuint,
}

impl NvInteropTexture {
    fn open(device: &ID3D11Device, texture: &ID3D11Texture2D) -> anyhow::Result<Self> {
        unsafe {
            let dx_device_handle = wgl::DXOpenDeviceNV(device.as_raw());
            if dx_device_handle.is_null() {
                bail!("DXOpenDeviceNV failed");
            }

            let mut gl_texture = 0;
            gl::GenTextures(1, &mut gl_texture);
            let mut last_id = 0;
            gl::GetIntegerv(gl::TEXTURE_BINDING_2D, &mut last_id);
            gl::BindTexture(gl::TEXTURE_2D, gl_texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
            gl::BindTexture(gl::TEXTURE_2D, last_id as _);

            let dx11_tex_handle = wgl::DXRegisterObjectNV(
                dx_device_handle,
                texture.as_raw(),
                gl_texture,
                gl::TEXTURE_2D,
                wgl::ACCESS_READ_ONLY_NV,
            );
            if dx11_tex_handle.is_null() {
                wgl::DXCloseDeviceNV(dx_device_handle as _);
                gl::DeleteTextures(1, &gl_texture);
                bail!("DXRegisterObjectNV failed");
            }

            Ok(NvInteropTexture {
                device_handle: dx_device_handle,
                dx11_tex_handle,
                gl_texture,
            })
        }
    }

    #[inline]
    fn bind(&self, target: gl::types::GLenum, f: impl FnOnce()) {
        unsafe {
            gl::BindTexture(target, self.gl_texture);
            wgl::DXLockObjectsNV(
                self.device_handle,
                1,
                &self.dx11_tex_handle as *const _ as _,
            );
            defer!({
                wgl::DXUnlockObjectsNV(
                    self.device_handle,
                    1,
                    &self.dx11_tex_handle as *const _ as _,
                );
            });

            f();
        }
    }
}

impl Drop for NvInteropTexture {
    fn drop(&mut self) {
        unsafe {
            wgl::DXUnregisterObjectNV(self.device_handle, self.dx11_tex_handle);
            gl::DeleteTextures(1, &self.gl_texture);
            wgl::DXCloseDeviceNV(self.device_handle as _);
        }
    }
}

unsafe impl Send for NvInteropTexture {}
