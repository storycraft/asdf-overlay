pub mod dx11;
pub mod dx12;
pub mod dx9;
pub mod opengl;

use core::num::NonZeroUsize;

use asdf_overlay_common::message::SharedHandle;
use dx9::Dx9Renderer;
use dx11::Dx11Renderer;
use dx12::Dx12Renderer;
use opengl::OpenglRenderer;
use parking_lot::Mutex;
use tracing::{debug, trace};
use windows::Win32::Foundation::{CloseHandle, HANDLE};

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
    #[tracing::instrument(skip(self))]
    pub fn update_texture(&mut self, shared: SharedHandle) {
        if let Some(ref mut renderer) = self.dx12 {
            renderer.update_texture(shared);
        } else if let Some(ref mut renderer) = self.dx11 {
            renderer.update_texture(shared);
        } else if let Some(ref mut renderer) = self.opengl {
            renderer.update_texture(shared);
        } else if let Some(ref mut renderer) = self.dx9 {
            renderer.update_texture(shared);
        }

        trace!("overlay texture updated");
    }

    #[inline]
    pub fn with<R>(f: impl FnOnce(&mut Renderers) -> R) -> R {
        f(&mut RENDERER.lock())
    }

    #[tracing::instrument(skip(self))]
    pub fn cleanup(&mut self) {
        {
            self.dx12.take();
            self.dx11.take();
            self.opengl.take();
            self.dx9.take();
        }

        debug!("renderer cleaned up");
    }
}

enum OverlayTextureState<T> {
    None,
    Handle(NonZeroUsize),
    Created(T),
}

impl<T> OverlayTextureState<T> {
    pub const fn new() -> Self {
        Self::None
    }

    pub fn map<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        if let Self::Created(ref created) = *self {
            Some(f(created))
        } else {
            None
        }
    }

    pub fn update(&mut self, shared: SharedHandle) {
        match shared.handle {
            Some(handle) => *self = Self::Handle(handle),
            None => *self = Self::None,
        }
    }

    pub fn get_or_create(
        &mut self,
        f: impl FnOnce(NonZeroUsize) -> anyhow::Result<Option<T>>,
    ) -> anyhow::Result<Option<&mut T>> {
        Ok(match *self {
            Self::None => None,

            Self::Handle(handle) => {
                if let Some(created) = f(handle)? {
                    *self = Self::Created(created);
                    let Self::Created(created) = self else {
                        unreachable!();
                    };

                    Some(created)
                } else {
                    *self = Self::None;
                    None
                }
            }

            Self::Created(ref mut created) => Some(created),
        })
    }
}

impl<T> Default for OverlayTextureState<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for OverlayTextureState<T> {
    fn drop(&mut self) {
        if let Self::Handle(handle) = self {
            unsafe { _ = CloseHandle(HANDLE(handle.get() as _)) };
        }
    }
}
