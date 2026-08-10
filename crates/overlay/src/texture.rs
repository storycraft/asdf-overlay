#[derive(Debug)]
pub enum OverlayTextureState<T> {
    None,
    Handle(u32),
    Created(T),
}

impl<T> OverlayTextureState<T> {
    pub const fn new() -> Self {
        Self::None
    }

    pub fn update(&mut self, handle: Option<u32>) {
        *self = match handle {
            Some(handle) => Self::Handle(handle),
            None => Self::None,
        }
    }

    pub fn get_or_create(
        &mut self,
        f: impl FnOnce(u32) -> anyhow::Result<Option<T>>,
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
