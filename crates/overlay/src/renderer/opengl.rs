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
    interop: GlInterop,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        unsafe {
            let interop = GlInterop::new(device)?;

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
                interop,

                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn update_texture(&mut self, texture: Option<&ID3D11Texture2D>) -> anyhow::Result<()> {
        let Some(texture) = texture else {
            return Ok(());
        };

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        let size = (desc.Width, desc.Height);
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }

        self.interop.open(texture, (size.0 as _, size.1 as _))?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        position: (i32, i32),
        size: (u32, u32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 || !self.interop.contains() {
            return Ok(());
        }

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
            self.interop.bind(gl::TEXTURE_2D, || {
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

enum GlInterop {
    MemoryObject {
        texture: Option<MemoryObjectTexture>,
    },
    Wgl {
        interop_texture: Option<NvInteropTexture>,
        dx_device_handle: *const c_void,
        texture_id: GLuint,
    },
}

impl GlInterop {
    pub fn new(device: &ID3D11Device) -> anyhow::Result<Self> {
        if gl::ImportMemoryWin32HandleEXT::is_loaded() {
            Ok(Self::MemoryObject { texture: None })
        } else if wgl::DXOpenDeviceNV::is_loaded() {
            let dx_device_handle = unsafe { wgl::DXOpenDeviceNV(device.as_raw()) };
            if dx_device_handle.is_null() {
                bail!("DXOpenDeviceNV failed");
            }

            let mut texture = 0;
            unsafe {
                gl::GenTextures(1, &mut texture);
                let mut last_id = 0;
                gl::GetIntegerv(gl::TEXTURE_BINDING_2D, &mut last_id);
                gl::BindTexture(gl::TEXTURE_2D, texture);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                gl::BindTexture(gl::TEXTURE_2D, last_id as _);
            }

            Ok(Self::Wgl {
                dx_device_handle,
                texture_id: texture,
                interop_texture: None,
            })
        } else {
            bail!("Opengl interop is not supported");
        }
    }

    #[inline]
    pub fn contains(&self) -> bool {
        match *self {
            Self::MemoryObject { ref texture } => texture.is_some(),
            Self::Wgl {
                ref interop_texture,
                ..
            } => interop_texture.is_some(),
        }
    }

    pub fn open(&mut self, texture: &ID3D11Texture2D, size: (u32, u32)) -> anyhow::Result<()> {
        // drop previous texture first, so it can be opened again without error
        self.take();

        match *self {
            Self::MemoryObject {
                texture: ref mut interop_texture,
            } => {
                *interop_texture = Some(MemoryObjectTexture::open(texture, size)?);
            }

            Self::Wgl {
                dx_device_handle,
                ref mut interop_texture,
                texture_id,
                ..
            } => {
                *interop_texture = Some(NvInteropTexture::open(
                    dx_device_handle,
                    texture,
                    texture_id,
                )?);
            }
        }

        Ok(())
    }

    pub fn take(&mut self) {
        match *self {
            Self::MemoryObject { ref mut texture } => {
                texture.take();
            }

            Self::Wgl {
                ref mut interop_texture,
                ..
            } => {
                interop_texture.take();
            }
        }
    }

    #[inline]
    pub fn bind(&self, target: gl::types::GLenum, f: impl FnOnce()) {
        match *self {
            Self::MemoryObject {
                texture: Some(ref texture),
            } => {
                unsafe { gl::BindTexture(target, texture.id) };
                f();
            }
            Self::Wgl {
                dx_device_handle,
                interop_texture: Some(ref interop_texture),
                texture_id,
            } => unsafe {
                gl::BindTexture(target, texture_id);
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
                f();
            },
            _ => {}
        }
    }
}

impl Drop for GlInterop {
    fn drop(&mut self) {
        if let GlInterop::Wgl {
            dx_device_handle,
            ref mut interop_texture,
            texture_id: ref texture,
        } = *self
        {
            interop_texture.take();
            unsafe {
                wgl::DXCloseDeviceNV(dx_device_handle as _);
                gl::DeleteTextures(1, texture);
            }
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

            gl::ImportMemoryWin32HandleEXT(
                memory_object,
                0,
                gl::HANDLE_TYPE_D3D11_IMAGE_KMT_EXT,
                handle,
            );

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

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);
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
    owned_device_handle: *const c_void,
    dx11_tex_handle: *const c_void,
}

impl NvInteropTexture {
    fn open(
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

            Ok(NvInteropTexture {
                owned_device_handle: dx_device_handle,
                dx11_tex_handle,
            })
        }
    }
}

impl Drop for NvInteropTexture {
    fn drop(&mut self) {
        unsafe {
            wgl::DXUnregisterObjectNV(self.owned_device_handle, self.dx11_tex_handle);
        }
    }
}

unsafe impl Send for NvInteropTexture {}
