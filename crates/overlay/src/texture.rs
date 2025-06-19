use core::num::NonZeroU32;

use asdf_overlay_common::request::UpdateSharedHandle;
use windows::Win32::Foundation::{CloseHandle, HANDLE};

#[derive(Debug)]
pub enum OverlayTextureState<T> {
    None,
    Handle(NonZeroU32),
    Created(T),
}

impl<T> OverlayTextureState<T> {
    pub const fn new() -> Self {
        Self::None
    }

    pub fn update(&mut self, shared: UpdateSharedHandle) {
        match shared.handle {
            Some(handle) => *self = Self::Handle(handle),
            None => *self = Self::None,
        }
    }

    pub fn get_or_create(
        &mut self,
        f: impl FnOnce(NonZeroU32) -> anyhow::Result<Option<T>>,
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
