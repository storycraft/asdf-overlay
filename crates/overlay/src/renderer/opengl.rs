pub mod data;

use anyhow::bail;
use asdf_overlay_common::request::UpdateSharedHandle;
use core::{ffi::c_void, mem, ptr};
use gl::types::{GLint, GLuint};
use scopeguard::defer;
use tracing::trace;
use windows::{
    Win32::{
        Foundation::{HANDLE, HMODULE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            },
        },
    },
    core::Interface,
};

use crate::{texture::OverlayTextureState, wgl};

#[derive(Clone, Copy)]
#[repr(C)]
struct Vertex {
    pub pos: (f32, f32),
}
type VertexArray = [Vertex; 4];
const VERTICES: VertexArray = [
    Vertex { pos: (0.0, 1.0) }, // bottom left
    Vertex { pos: (0.0, 0.0) }, // top left
    Vertex { pos: (1.0, 1.0) }, // bottom right
    Vertex { pos: (1.0, 0.0) }, // top right
];

static VERTEX_SHADER: &str = include_str!("opengl/shaders/texture.vert");
static FRAGMENT_SHADER: &str = include_str!("opengl/shaders/texture.frag");

pub struct OpenglRenderer {
    dx_device_handle: *mut c_void,
    dx_device: ID3D11Device,
    _dx_cx: ID3D11DeviceContext,

    state: OverlayTextureState<Tex>,

    vertex_buffer: GLuint,
    vao: GLuint,
    texture: GLuint,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new() -> anyhow::Result<Self> {
        unsafe {
            let mut dx_device = None;
            let mut dx_cx = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut dx_device),
                None,
                Some(&mut dx_cx),
            )?;
            let dx_device = dx_device.unwrap();
            let dx_device_handle = wgl::DXOpenDeviceNV(dx_device.as_raw()).cast_mut();
            if dx_device_handle.is_null() {
                bail!("DXOpenDeviceNV failed");
            }

            let dx_cx = dx_cx.unwrap();

            let mut vertex_buffer = 0;
            gl::GenBuffers(1, &mut vertex_buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, vertex_buffer);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                mem::size_of::<VertexArray>() as _,
                &VERTICES as *const _ as _,
                gl::STATIC_DRAW,
            );

            let mut vao = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE,
                mem::size_of::<Vertex>() as _,
                ptr::null::<c_void>().with_addr(mem::offset_of!(Vertex, pos)),
            );
            gl::EnableVertexAttribArray(0);

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
                dx_device,
                _dx_cx: dx_cx,

                state: OverlayTextureState::new(),

                vertex_buffer,
                vao,
                texture,
                program,
                rect_loc,
                tex_loc,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.state.map(|tex| tex.size).unwrap_or((0, 0))
    }

    pub fn update_texture(&mut self, shared: UpdateSharedHandle) {
        self.state.update(shared);
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(&mut self, position: (f32, f32), screen: (u32, u32)) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let Some(Tex {
            size,
            dx11_tex_handle,
            ..
        }) = self.state.get_or_create(|handle| {
            let mut texture = None;
            unsafe {
                self.dx_device.OpenSharedResource::<ID3D11Texture2D>(
                    HANDLE(handle.get() as _),
                    &mut texture,
                )?;
                let texture = texture.unwrap();

                let mut desc = D3D11_TEXTURE2D_DESC::default();
                texture.GetDesc(&mut desc);
                let size = (desc.Width, desc.Height);
                if size.0 == 0 || size.1 == 0 {
                    return Ok(None);
                }

                let dx11_tex_handle = wgl::DXRegisterObjectNV(
                    self.dx_device_handle,
                    texture.into_raw(),
                    self.texture,
                    gl::TEXTURE_2D,
                    wgl::ACCESS_READ_ONLY_NV,
                );
                if dx11_tex_handle.is_null() {
                    bail!("DXRegisterObjectNV failed");
                }

                Ok(Some(Tex {
                    size,
                    owned_device_handle: self.dx_device_handle,
                    dx11_tex_handle,
                }))
            }
        })?
        else {
            return Ok(());
        };

        let rect: [f32; 4] = [
            (position.0 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 / screen.1 as f32) * 2.0 + 1.0,
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

            gl::BindBuffer(gl::ARRAY_BUFFER, self.vertex_buffer);
            gl::BindVertexArray(self.vao);
            gl::UseProgram(self.program);

            wgl::DXLockObjectsNV(dx_device_handle, 1, dx11_tex_handle as *mut _);
            defer!({
                wgl::DXUnlockObjectsNV(dx_device_handle, 1, dx11_tex_handle as *mut _);
            });

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);

            gl::Uniform4f(self.rect_loc, rect[0], rect[1], rect[2], rect[3]);
            gl::Uniform1i(self.tex_loc, 0);

            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        Ok(())
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        self.state = OverlayTextureState::None;

        unsafe {
            wgl::DXCloseDeviceNV(self.dx_device_handle as _);

            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vertex_buffer);
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteProgram(self.program);
        }
        trace!("OpenGL resources freed");
    }
}

unsafe impl Send for OpenglRenderer {}
unsafe impl Sync for OpenglRenderer {}

struct Tex {
    size: (u32, u32),
    owned_device_handle: *mut c_void,
    dx11_tex_handle: *const c_void,
}

impl Drop for Tex {
    fn drop(&mut self) {
        unsafe {
            wgl::DXUnregisterObjectNV(self.owned_device_handle, self.dx11_tex_handle);
        }
    }
}

unsafe impl Send for Tex {}
