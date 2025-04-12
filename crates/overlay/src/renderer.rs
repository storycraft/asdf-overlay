pub mod dx11;
pub mod dx12;
pub mod dx9;
pub mod opengl;

use asdf_overlay_common::message::Bitmap;
use dx9::Dx9Renderer;
use dx11::Dx11Renderer;
use dx12::Dx12Renderer;
use opengl::OpenglRenderer;
use parking_lot::Mutex;

static RENDERER: Mutex<Renderers> = Mutex::new(Renderers {
    dx12: None,
    dx11: None,
    opengl: None,
    dx9: None,
});

pub struct Renderers {
    pub dx12: Option<Dx12Renderer>,
    pub dx11: Option<Dx11Renderer>,
    pub opengl: Option<OpenglRenderer>,
    pub dx9: Option<Dx9Renderer>,
}

impl Renderers {
    pub fn update_texture(&mut self, bitmap: Bitmap) {
        if let Some(ref mut renderer) = self.dx12 {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = self.dx11 {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = self.opengl {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = self.dx9 {
            renderer.update_texture(bitmap.width, bitmap.data);
        }
    }

    #[inline]
    pub fn with<R>(f: impl FnOnce(&mut Renderers) -> R) -> R {
        f(&mut RENDERER.lock())
    }

    pub fn cleanup(&mut self) {
        self.dx12.take();
        self.dx11.take();
        self.opengl.take();
        self.dx9.take();
    }
}
