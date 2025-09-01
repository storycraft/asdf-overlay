//! Size-related types and utilities.

use bincode::{Decode, Encode};

/// A length that can be specified as either a percentage of a container size or an absolute length.
#[derive(Debug, Decode, Encode, Clone, Copy, PartialEq)]
pub enum PercentLength {
    /// A percentage length relative to a base size.
    ///
    /// For example, `0.5` is 50% of the base size.
    Percent(f32),

    /// An absolute length. Usually in pixels.
    ///
    /// For example, `100.0` is 100 pixels.
    Length(f32),
}

impl PercentLength {
    pub const ZERO: Self = Self::Length(0.0);

    /// Resolves the [`PercentLength`] to an absolute length based on the given container size.
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
