use asdf_overlay_common::{
    request::{SetAnchor, SetMargin, SetPosition},
    size::PercentLength,
};

#[derive(Clone)]
pub struct OverlayLayout {
    pub position: SetPosition,
    pub anchor: SetAnchor,
    pub margin: SetMargin,
}

impl OverlayLayout {
    pub const fn new() -> Self {
        Self {
            position: SetPosition {
                x: PercentLength::ZERO,
                y: PercentLength::ZERO,
            },
            anchor: SetAnchor {
                x: PercentLength::ZERO,
                y: PercentLength::ZERO,
            },
            margin: SetMargin {
                top: PercentLength::ZERO,
                right: PercentLength::ZERO,
                bottom: PercentLength::ZERO,
                left: PercentLength::ZERO,
            },
        }
    }

    pub fn calc_position(&self, size: (f32, f32), screen: (u32, u32)) -> (f32, f32) {
        let screen = (screen.0 as f32, screen.1 as f32);

        let margin_left = self.margin.left.resolve(screen.0);
        let margin_top = self.margin.top.resolve(screen.1);

        let outer_width = margin_left + size.0 + self.margin.right.resolve(screen.0);
        let outer_height = margin_top + size.1 + self.margin.bottom.resolve(screen.1);

        let x =
            self.position.x.resolve(screen.0) - self.anchor.x.resolve(outer_width) + margin_left;
        let y =
            self.position.y.resolve(screen.1) - self.anchor.y.resolve(outer_height) + margin_top;

        (x, y)
    }
}

impl Default for OverlayLayout {
    fn default() -> Self {
        Self::new()
    }
}
