use dashmap::DashMap;
use once_cell::sync::Lazy;
use rustc_hash::FxBuildHasher;
use tracing::debug;
use windows::Win32::Foundation::HWND;

use crate::renderer::{
    dx9::Dx9Renderer, dx11::Dx11Renderer, dx12::Dx12Renderer, opengl::OpenglRenderer,
};

static BACKENDS: Lazy<Backends> = Lazy::new(|| Backends {
    map: DashMap::default(),
});

pub struct Backends {
    map: DashMap<usize, WindowBackend, FxBuildHasher>,
}

impl Backends {
    pub fn with_backend<R>(hwnd: HWND, f: impl FnOnce(&mut WindowBackend) -> R) -> Option<R> {
        let Some(mut backend) = BACKENDS.map.get_mut(&(hwnd.0 as usize)) else {
            return None;
        };

        Some(f(&mut *backend))
    }

    pub fn with_or_init_backend<R>(
        hwnd: HWND,
        f: impl FnOnce(&mut WindowBackend) -> R,
    ) -> anyhow::Result<R> {
        let mut backend = BACKENDS.map.entry(hwnd.0 as usize).or_try_insert_with(|| {
            Ok::<_, anyhow::Error>(WindowBackend {
                renderers: Renderers {
                    dx12: None,
                    dx11: None,
                    opengl: None,
                    dx9: None,
                },
            })
        })?;

        Ok(f(&mut backend))
    }

    #[tracing::instrument()]
    pub fn cleanup() {
        BACKENDS.map.clear();
        debug!("backends cleaned up");
    }
}

pub struct WindowBackend {
    pub renderers: Renderers,
}

pub struct Renderers {
    pub dx12: Option<Dx12Renderer>,
    pub dx11: Option<Dx11Renderer>,
    pub opengl: Option<OpenglRenderer>,
    pub dx9: Option<Dx9Renderer>,
}
