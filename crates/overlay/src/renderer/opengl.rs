use core::ffi::c_void;
use std::collections::HashSet;

use crate::{
    gl::{
        self,
        types::{GLint, GLuint},
    },
    surface::{SharedTextureHandle, texture::OverlaySurface},
    wgl,
};
use anyhow::{Context, bail};
use scopeguard::{ScopeGuard, defer};
use tracing::{Level, trace};
use windows::{
    Win32::Graphics::{
        Direct3D11::ID3D11Device,
        Dxgi::Common::{
            DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_UNORM,
        },
    },
    core::Interface,
};

static VERTEX_SHADER: &str = include_str!("opengl/shaders/texture.vert");
static FRAGMENT_SHADER: &str = include_str!("opengl/shaders/texture.frag");

pub struct OpenglRenderer {
    interop: Option<GlInteropTexture>,
    vao: GLuint,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument(level = Level::DEBUG)]
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            let mut vao = 0;
            gl::GenVertexArrays(1, &mut vao);
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
                vao,
                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn update_texture(
        &mut self,
        device: &ID3D11Device,
        surface: Option<&OverlaySurface>,
        extensions: &HashSet<String>,
    ) -> anyhow::Result<()> {
        self.interop.take();
        let Some(surface) = surface else {
            return Ok(());
        };

        let size = surface.size();
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }

        self.interop = Some(GlInteropTexture::new(device, surface, extensions)?);
        Ok(())
    }

    #[tracing::instrument(level = Level::TRACE, skip(self))]
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
            gl::Disable(gl::FRAMEBUFFER_SRGB);

            gl::BindVertexArray(self.vao);
            gl::UseProgram(self.program);
            gl::Uniform4f(self.rect_loc, rect[0], rect[1], rect[2], rect[3]);
            gl::Uniform1i(self.tex_loc, 0);

            texture.bind(gl::TEXTURE_2D, || {
                gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
            });
        }

        Ok(())
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(level = Level::TRACE, skip(self))]
    fn drop(&mut self) {
        self.interop.take();
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
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
    pub fn new(
        device: &ID3D11Device,
        surface: &OverlaySurface,
        extensions: &HashSet<String>,
    ) -> anyhow::Result<Self> {
        if extensions.contains("GL_EXT_memory_object_win32")
            && gl::ImportMemoryWin32HandleEXT::is_loaded()
        {
            return Ok(Self::MemoryObject(
                MemoryObjectTexture::open(surface, extensions)
                    .context("external memory texture")?,
            ));
        }

        if extensions.contains("WGL_NV_DX_interop2") && wgl::DXOpenDeviceNV::is_loaded() {
            return Ok(Self::Wgl(
                NvInteropTexture::open(device, surface).context("NV interop texture")?,
            ));
        }

        bail!("Opengl interop is not supported");
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
    keyed_mutex: bool,
    id: GLuint,
}

impl MemoryObjectTexture {
    fn open(surface: &OverlaySurface, extensions: &HashSet<String>) -> anyhow::Result<Self> {
        unsafe {
            let memory_object = scopeguard::guard(
                {
                    let mut id = 0;
                    gl::CreateMemoryObjectsEXT(1, &mut id);
                    id
                },
                |value| {
                    gl::DeleteMemoryObjectsEXT(1, &value);
                },
            );

            // reset previous error before
            _ = gl::GetError();
            let handle = surface.shared_handle();
            gl::ImportMemoryWin32HandleEXT(
                *memory_object,
                0,
                match handle {
                    SharedTextureHandle::Kmt(_) => gl::HANDLE_TYPE_D3D11_IMAGE_KMT_EXT,
                    SharedTextureHandle::Nt(_) => gl::HANDLE_TYPE_D3D11_IMAGE_EXT,
                },
                handle.as_raw() as _,
            );
            if gl::GetError() != gl::NO_ERROR {
                bail!("ImportMemoryWin32HandleEXT failed");
            }

            let texture = scopeguard::guard(
                {
                    let mut id = 0;
                    gl::GenTextures(1, &mut id);
                    id
                },
                |value| {
                    gl::DeleteTextures(1, &value);
                },
            );
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, *texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
            let (width, height) = surface.size();
            let (internal_format, swizzling) =
                map_dxgi_to_gl(surface.format()).context("Unsupported DXGI format")?;
            gl::TexStorageMem2DEXT(
                gl::TEXTURE_2D,
                1,
                internal_format,
                width as _,
                height as _,
                *memory_object,
                0,
            );
            if gl::GetError() != gl::NO_ERROR {
                bail!("TexStorageMem2DEXT failed");
            }

            if let Some(swizzling) = swizzling {
                gl::TexParameterIuiv(gl::TEXTURE_2D, gl::TEXTURE_SWIZZLE_RGBA, swizzling.as_ptr());
            }

            Ok(Self {
                memory_object: ScopeGuard::into_inner(memory_object),
                keyed_mutex: surface.mutex().is_some()
                    && extensions.contains("GL_EXT_win32_keyed_mutex")
                    && gl::AcquireKeyedMutexWin32EXT::is_loaded(),
                id: ScopeGuard::into_inner(texture),
            })
        }
    }

    #[inline]
    pub fn bind(&self, target: gl::types::GLenum, f: impl FnOnce()) {
        unsafe {
            if !self.keyed_mutex {
                gl::BindTexture(target, self.id);
                f();
                return;
            }

            gl::AcquireKeyedMutexWin32EXT(self.memory_object, 0, u32::MAX);
            defer!({
                _ = gl::ReleaseKeyedMutexWin32EXT(self.memory_object, 0);
            });
            gl::BindTexture(target, self.id);
            f();
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

struct NvInteropTexture {
    device_handle: *const c_void,
    dx11_tex_handle: *const c_void,
    gl_texture: GLuint,
}

impl NvInteropTexture {
    fn open(device: &ID3D11Device, surface: &OverlaySurface) -> anyhow::Result<Self> {
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
                surface.texture().as_raw() as _,
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

fn map_dxgi_to_gl(format: DXGI_FORMAT) -> Option<(GLuint, Option<[GLuint; 4]>)> {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => Some((gl::RGBA8, None)),
        DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => {
            Some((gl::RGBA8, Some([gl::BLUE, gl::GREEN, gl::RED, gl::ALPHA])))
        }
        DXGI_FORMAT_R16G16B16A16_UNORM => Some((gl::RGBA16, None)),
        DXGI_FORMAT_R16G16B16A16_FLOAT => Some((gl::RGBA16F, None)),
        _ => None,
    }
}
