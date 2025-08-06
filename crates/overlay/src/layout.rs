use asdf_overlay_common::size::PercentLength;

#[derive(Clone)]
pub struct OverlayLayout {
    position: (PercentLength, PercentLength),
    anchor: (PercentLength, PercentLength),
    margin: (PercentLength, PercentLength, PercentLength, PercentLength),
}

impl OverlayLayout {
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

    #[inline]
    pub fn set_position(&mut self, x: PercentLength, y: PercentLength) {
        self.position = (x, y);
    }

    #[inline]
    pub fn set_anchor(&mut self, x: PercentLength, y: PercentLength) {
        self.anchor = (x, y);
    }

    #[inline]
    pub fn set_margin(
        &mut self,
        top: PercentLength,
        right: PercentLength,
        bottom: PercentLength,
        left: PercentLength,
    ) {
        self.margin = (top, right, bottom, left);
    }

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
