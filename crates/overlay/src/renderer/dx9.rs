use core::{
    mem,
    ptr::{self, copy_nonoverlapping},
};

use anyhow::Context;
use scopeguard::defer;
use windows::Win32::Graphics::{Direct3D9::*, Direct3D11::D3D11_MAPPED_SUBRESOURCE};

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

pub struct Dx9Renderer {
    size: (u32, u32),
    texture_size: (u32, u32),

    texture: Option<IDirect3DTexture9>,
    state_block: IDirect3DStateBlock9,
}

impl Dx9Renderer {
    #[tracing::instrument]
    pub fn new(device: &IDirect3DDevice9) -> anyhow::Result<Self> {
        unsafe {
            let state_block = device.CreateStateBlock(D3DSBT_ALL)?;
            Ok(Self {
                texture_size: (2, 2),
                size: (0, 0),

                texture: None,
                state_block,
            })
        }
    }

    #[inline]
    pub fn reset_texture(&mut self) {
        self.texture.take();
    }

    pub fn update_texture(
        &mut self,
        device: &IDirect3DDevice9,
        size: (u32, u32),
        mapped: &D3D11_MAPPED_SUBRESOURCE,
    ) -> anyhow::Result<()> {
        if self.size != size {
            self.reset_texture();
        }

        let texture = match self.texture {
            Some(ref mut texture) => texture,
            None => {
                self.size = size;
                self.texture_size = (size.0.next_power_of_two(), size.1.next_power_of_two());
                let mut texture = None;
                unsafe {
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
            }
        };

        let mut rect = D3DLOCKED_RECT::default();
        unsafe {
            texture.LockRect(0, &mut rect, ptr::null(), D3DLOCK_DISCARD as _)?;
            defer!({
                _ = texture.UnlockRect(0);
            });

            for y in 0..size.1 as isize {
                let line_size = size.0 as usize * 4;
                let src_offset = y * mapped.RowPitch as isize;
                let dest_offset = y * rect.Pitch as isize;

                copy_nonoverlapping(
                    mapped.pData.cast::<u8>().byte_offset(src_offset),
                    rect.pBits.cast::<u8>().byte_offset(dest_offset),
                    line_size,
                );
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn draw(
        &mut self,
        device: &IDirect3DDevice9,
        position: (f32, f32),
        screen: (u32, u32),
    ) -> anyhow::Result<()> {
        if screen.0 == 0 || screen.1 == 0 {
            return Ok(());
        }

        let Some(ref texture) = self.texture else {
            return Ok(());
        };

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
                Vertex::new((pos.0, pos.1 + size.1), (0.0, texture_size.1)), // bottom left
                Vertex::new(pos, (0.0, 0.0)),                                // top left
                Vertex::new(
                    (pos.0 + size.0, pos.1 + size.1),
                    (texture_size.0, texture_size.1),
                ), // bottom right
                Vertex::new((pos.0 + size.0, pos.1), (texture_size.0, 0.0)), // top right
            ]
        };

        unsafe {
            let state_block = &self.state_block;
            state_block.Capture()?;
            defer!({
                _ = state_block.Apply();
            });

            // disable srgb gamma correction enabled in some games
            device.SetRenderState(D3DRS_SRGBWRITEENABLE, 0)?;
            device.SetRenderState(D3DRS_ALPHABLENDENABLE, 1)?;
            device.SetRenderState(D3DRS_SRCBLEND, D3DBLEND_SRCALPHA.0 as _)?;
            device.SetRenderState(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA.0 as _)?;
            device.SetTextureStageState(0, D3DTSS_COLOROP, D3DTSS_COLORARG1.0 as _)?;
            device.SetTextureStageState(0, D3DTSS_COLORARG1, D3DTA_TEXTURE)?;

            device.SetFVF(Vertex::FVF)?;
            device.SetTexture(0, texture)?;
            device.DrawPrimitiveUP(
                D3DPT_TRIANGLESTRIP,
                2,
                vertices.as_ptr().cast(),
                mem::size_of::<Vertex>() as _,
            )?;

            Ok(())
        }
    }
}

unsafe impl Send for Dx9Renderer {}
unsafe impl Sync for Dx9Renderer {}
