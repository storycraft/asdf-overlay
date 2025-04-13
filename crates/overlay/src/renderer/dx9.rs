use core::{
    mem,
    ptr::{self, copy_nonoverlapping},
};

use anyhow::Context;
use scopeguard::defer;
use windows::Win32::Graphics::Direct3D9::{
    D3DBLEND_INVSRCALPHA, D3DBLEND_SRCALPHA, D3DFMT_A8R8G8B8, D3DFVF_TEX1, D3DFVF_XYZW,
    D3DLOCK_DISCARD, D3DLOCKED_RECT, D3DPOOL_DEFAULT, D3DPT_TRIANGLEFAN, D3DRS_ALPHABLENDENABLE,
    D3DRS_DESTBLEND, D3DRS_SRCBLEND, D3DRS_SRGBWRITEENABLE, D3DSBT_ALL, D3DUSAGE_DYNAMIC,
    D3DUSAGE_WRITEONLY, IDirect3DDevice9, IDirect3DStateBlock9, IDirect3DTexture9,
    IDirect3DVertexBuffer9,
};

#[derive(Clone, Copy)]
#[repr(C)]
struct Vertex {
    pub pos: (f32, f32),
    pub pos_z: f32,
    pub rhw: f32,
    pub texture_pos: (f32, f32),
}

impl Vertex {
    const FVF: u32 = D3DFVF_XYZW | D3DFVF_TEX1;

    const fn new(pos: (f32, f32), texture_pos: (f32, f32)) -> Self {
        Self {
            pos,
            pos_z: 0.0,
            rhw: 1.0,
            texture_pos,
        }
    }
}

type VertexArray = [Vertex; 4];

pub struct Dx9Renderer {
    texture_size: (u32, u32),
    size: (u32, u32),
    data: Vec<u8>,

    vertex_buffer: IDirect3DVertexBuffer9,
    texture: Option<IDirect3DTexture9>,
    texture_outdated: bool,
    state_block: IDirect3DStateBlock9,
}

impl Dx9Renderer {
    #[tracing::instrument]
    pub fn new(device: &IDirect3DDevice9) -> anyhow::Result<Self> {
        unsafe {
            let mut vertex_buffer = None;
            device.CreateVertexBuffer(
                mem::size_of::<VertexArray>() as _,
                (D3DUSAGE_WRITEONLY | D3DUSAGE_DYNAMIC) as _,
                Vertex::FVF,
                D3DPOOL_DEFAULT,
                &mut vertex_buffer,
                ptr::null_mut(),
            )?;
            let vertex_buffer = vertex_buffer.context("cannot create vertex buffer")?;

            let state_block = device.CreateStateBlock(D3DSBT_ALL)?;
            Ok(Self {
                texture_size: (2, 2),
                size: (0, 0),
                data: Vec::new(),

                vertex_buffer,
                texture: None,
                texture_outdated: true,
                state_block,
            })
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
            self.texture.take();
            self.size = size;
            self.texture_size = (size.0.next_power_of_two(), size.1.next_power_of_two());
        }

        self.data = data;
        self.texture_outdated = true;
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &IDirect3DDevice9,
        position: (f32, f32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        let vertices = {
            let pos = (
                (position.0 / screen.0 as f32) * 2.0 - 1.0,
                -(position.1 / screen.1 as f32) * 2.0 + 1.0,
            );
            let size = (
                (self.size.0 as f32 / screen.0 as f32) * 2.0,
                -(self.size.1 as f32 / screen.1 as f32) * 2.0,
            );
            let texture_size = (
                self.size.0 as f32 / self.texture_size.0 as f32,
                self.size.1 as f32 / self.texture_size.1 as f32,
            );

            [
                Vertex::new(pos, (0.0, 0.0)),
                Vertex::new((pos.0 + size.0, pos.1), (texture_size.0, 0.0)),
                Vertex::new(
                    (pos.0 + size.0, pos.1 + size.1),
                    (texture_size.0, texture_size.1),
                ),
                Vertex::new((pos.0, pos.1 + size.1), (0.0, texture_size.1)),
            ]
        };

        unsafe {
            let state_block = &self.state_block;
            state_block.Capture()?;
            defer!({
                _ = state_block.Apply();
            });

            let texture = match self.texture {
                Some(ref texture) => texture,
                None => {
                    // no non-power-of-two texture support
                    let mut texture = None;
                    device.CreateTexture(
                        self.texture_size.0,
                        self.texture_size.1,
                        1,
                        D3DUSAGE_DYNAMIC as _,
                        D3DFMT_A8R8G8B8,
                        D3DPOOL_DEFAULT,
                        &mut texture,
                        ptr::null_mut(),
                    )?;
                    self.texture
                        .insert(texture.context("cannot create texture")?)
                }
            };

            if self.texture_outdated {
                self.texture_outdated = false;

                let mut rect = D3DLOCKED_RECT::default();
                texture.LockRect(0, &mut rect, ptr::null(), D3DLOCK_DISCARD as _)?;
                defer!({
                    _ = texture.UnlockRect(0);
                });

                for y in 0..self.size.1 as isize {
                    let line_size = self.size.0 as usize * 4;
                    let src_offset = y * line_size as isize;
                    let dest_offset = y * rect.Pitch as isize;

                    copy_nonoverlapping(
                        self.data.as_ptr().offset(src_offset),
                        rect.pBits.cast::<u8>().byte_offset(dest_offset),
                        line_size,
                    );
                }
            }

            {
                let vertex_buffer = &self.vertex_buffer;
                let mut ptr = ptr::null_mut();
                vertex_buffer.Lock(
                    0,
                    mem::size_of::<VertexArray>() as _,
                    &mut ptr,
                    D3DLOCK_DISCARD as _,
                )?;
                defer!({
                    _ = vertex_buffer.Unlock();
                });

                ptr.cast::<VertexArray>().write(vertices);
            }

            // disable srgb gamma correction enabled in some games
            device.SetRenderState(D3DRS_SRGBWRITEENABLE, 0)?;
            device.SetRenderState(D3DRS_ALPHABLENDENABLE, 1)?;
            device.SetRenderState(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA.0 as _)?;
            device.SetRenderState(D3DRS_SRCBLEND, D3DBLEND_SRCALPHA.0 as _)?;

            device.SetStreamSource(0, &self.vertex_buffer, 0, mem::size_of::<Vertex>() as _)?;
            device.SetFVF(Vertex::FVF)?;
            device.SetTexture(0, texture)?;
            device.DrawPrimitive(D3DPT_TRIANGLEFAN, 0, 2)?;

            Ok(())
        }
    }
}

unsafe impl Send for Dx9Renderer {}
