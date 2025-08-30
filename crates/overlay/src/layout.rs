//! Minimal overlay surface layout implementation for smooth window resizing.
//! If you need more advanced layout system, create a fullscreen overlay surface and bring your own layout system.

use asdf_overlay_common::size::PercentLength;

#[derive(Clone)]
/// A simple overlay layout implementation.
pub struct OverlayLayout {
    /// X, Y Position of the overlay surface, relative to window's client size.
    pub position: (PercentLength, PercentLength),
    /// X, Y Positioning anchor of the overlay surface, relative to overlay's surface size.
    pub anchor: (PercentLength, PercentLength),
    /// Top, right, bottom, left margins of the overlay surface, relative to window's client size.
    pub margin: (PercentLength, PercentLength, PercentLength, PercentLength),
}

impl OverlayLayout {
    /// Create a new default [`OverlayLayout`].
    pub const fn new() -> Self {
        Self {
            position: (PercentLength::ZERO, PercentLength::ZERO),
            anchor: (PercentLength::ZERO, PercentLength::ZERO),
            margin: (
                PercentLength::ZERO,
                PercentLength::ZERO,
                PercentLength::ZERO,
                PercentLength::ZERO,
            ),
        }
    }

    /// Calculate absolute position of the window relative to `screen`,
    /// pretends overlay surface to have `size` size.
    pub fn calc(&self, size: (u32, u32), screen: (u32, u32)) -> (i32, i32) {
        let size = (size.0 as f32, size.1 as f32);
        let screen = (screen.0 as f32, screen.1 as f32);
        let (x, y) = self.position;
        let (anchor_x, anchor_y) = self.anchor;
        let (margin_top, margin_right, margin_bottom, margin_left) = self.margin;

        let margin_left = margin_left.resolve(screen.0);
        let margin_top = margin_top.resolve(screen.1);

        let outer_width = margin_left + size.0 + margin_right.resolve(screen.0);
        let outer_height = margin_top + size.1 + margin_bottom.resolve(screen.1);

        let x = x.resolve(screen.0) - anchor_x.resolve(outer_width) + margin_left;
        let y = y.resolve(screen.1) - anchor_y.resolve(outer_height) + margin_top;

        (x.round() as i32, y.round() as i32)
    }
}

impl Default for OverlayLayout {
    fn default() -> Self {
        Self::new()
    }
}
