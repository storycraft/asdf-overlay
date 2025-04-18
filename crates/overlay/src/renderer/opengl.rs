use core::{ffi::c_void, mem, ptr};
use gl::types::{GLint, GLuint};
use tracing::trace;

#[derive(Clone, Copy)]
#[repr(C)]
struct Vertex {
    pub pos: (f32, f32),
}
type VertexArray = [Vertex; 4];
const VERTICES: VertexArray = [
    Vertex { pos: (0.0, 0.0) },
    Vertex { pos: (1.0, 0.0) },
    Vertex { pos: (1.0, 1.0) },
    Vertex { pos: (0.0, 1.0) },
];

static VERTEX_SHADER: &str = include_str!("opengl/shaders/texture.vert");
static FRAGMENT_SHADER: &str = include_str!("opengl/shaders/texture.frag");

pub struct OpenglRenderer {
    size: (u32, u32),
    data: Vec<u8>,
    texture_size_outdated: bool,
    texture_outdated: bool,

    vertex_buffer: GLuint,
    vao: GLuint,
    texture: GLuint,
    program: GLuint,
    rect_loc: GLint,
    tex_loc: GLint,
}

impl OpenglRenderer {
    #[tracing::instrument]
    pub fn new() -> Self {
        unsafe {
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::Enable(gl::BLEND);

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
            gl::BindTexture(gl::TEXTURE_2D, texture);

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

            gl::UseProgram(program);

            let rect_loc = gl::GetUniformLocation(program, b"rect\0" as *const _ as _);
            let tex_loc = gl::GetUniformLocation(program, b"tex\0" as *const _ as _);

            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            Self {
                size: (0, 0),
                data: Vec::new(),
                texture_size_outdated: true,
                texture_outdated: true,

                vertex_buffer,
                vao,
                texture,
                program,
                rect_loc,
                tex_loc,
            }
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn update_texture(&mut self, width: u32, data: Vec<u8>) {
        if width == 0 || data.len() < width as _ {
            return;
        }

        let size = (width, (data.len() / width as usize / 4) as u32);

        if self.size != size {
            self.texture_size_outdated = true;
        }

        self.size = size;
        self.data = data;
        self.texture_outdated = true;
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(&mut self, position: (f32, f32), screen: (u32, u32)) {
        if self.size.0 == 0 || self.size.1 == 0 || screen.0 == 0 || screen.1 == 0 {
            return;
        }

        if self.texture_size_outdated {
            self.texture_size_outdated = false;

            unsafe {
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::BGRA as _,
                    self.size.0 as _,
                    self.size.1 as _,
                    0,
                    gl::BGRA,
                    gl::UNSIGNED_BYTE,
                    ptr::null(),
                );
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            }
        }

        if self.texture_outdated {
            self.texture_outdated = false;

            unsafe {
                gl::TexSubImage2D(
                    gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    self.size.0 as _,
                    self.size.1 as _,
                    gl::BGRA,
                    gl::UNSIGNED_BYTE,
                    &self.data[..] as *const _ as _,
                );
            }
        }

        let rect: [f32; 4] = [
            (position.0 / screen.0 as f32) * 2.0 - 1.0,
            -(position.1 / screen.1 as f32) * 2.0 + 1.0,
            (self.size.0 as f32 / screen.0 as f32) * 2.0,
            -(self.size.1 as f32 / screen.1 as f32) * 2.0,
        ];

        unsafe {
            gl::Viewport(0, 0, screen.0 as _, screen.1 as _);

            gl::Uniform4f(self.rect_loc, rect[0], rect[1], rect[2], rect[3]);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::Uniform1i(self.tex_loc, 0);

            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }
}

impl Drop for OpenglRenderer {
    #[tracing::instrument(skip(self))]
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vertex_buffer);
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteProgram(self.program);
        }
        trace!("OpenGL resources freed");
    }
}
