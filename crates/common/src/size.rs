use bincode::{Decode, Encode};

#[derive(Debug, Decode, Encode, Clone, Copy, PartialEq)]
pub enum PercentLength {
    Percent(f32),
    Length(f32),
}

impl PercentLength {
    pub const ZERO: Self = Self::Length(0.0);

    pub const fn resolve(self, container_size: f32) -> f32 {
        match self {
            Self::Percent(percent) => container_size * percent,
            Self::Length(length) => length,
        }
    }
}

impl Default for PercentLength {
    fn default() -> Self {
        Self::ZERO
    }
}
