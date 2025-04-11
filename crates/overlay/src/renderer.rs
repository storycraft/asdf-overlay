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

static RENDERER: Renderers = Renderers {
    dx12: Mutex::new(None),
    dx11: Mutex::new(None),
    opengl: Mutex::new(None),
    dx9: Mutex::new(None),
};

pub struct Renderers {
    pub dx12: Mutex<Option<Dx12Renderer>>,
    pub dx11: Mutex<Option<Dx11Renderer>>,
    pub opengl: Mutex<Option<OpenglRenderer>>,
    pub dx9: Mutex<Option<Dx9Renderer>>,
}

impl Renderers {
    pub fn update_texture(&self, bitmap: Bitmap) {
        if let Some(ref mut renderer) = *self.dx12.lock() {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = *self.dx11.lock() {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = *self.opengl.lock() {
            renderer.update_texture(bitmap.width, bitmap.data);
        } else if let Some(ref mut renderer) = *self.dx9.lock() {
            renderer.update_texture(bitmap.width, bitmap.data);
        }
    }

    #[inline]
    pub fn get() -> &'static Renderers {
        &RENDERER
    }

    pub fn cleanup(&self) {
        self.dx12.lock().take();
        self.dx11.lock().take();
        self.opengl.lock().take();
        self.dx9.lock().take();
    }
}
