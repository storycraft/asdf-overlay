use core::num::NonZeroU8;

use bincode::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode, Copy, Hash, PartialEq, Eq)]
pub struct Key {
    pub code: NonZeroU8,
    pub extended: bool,
}

impl Key {
    pub fn new(code: u8, extended: bool) -> Option<Self> {
        NonZeroU8::new(code).map(|code| Key { code, extended })
    }
}
