use asdf_overlay_common::size::PercentLength;

#[derive(Clone)]
pub struct OverlayLayout {
    position: (PercentLength, PercentLength),
    anchor: (PercentLength, PercentLength),
    margin: (PercentLength, PercentLength, PercentLength, PercentLength),
    cache: Option<LayoutCache>,
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
            cache: None,
        }
    }

    pub fn set_position(&mut self, x: PercentLength, y: PercentLength) {
        self.cache.take();
        self.position = (x, y);
    }

    pub fn set_anchor(&mut self, x: PercentLength, y: PercentLength) {
        self.cache.take();
        self.anchor = (x, y);
    }

    pub fn set_margin(
        &mut self,
        top: PercentLength,
        right: PercentLength,
        bottom: PercentLength,
        left: PercentLength,
    ) {
        self.cache.take();
        self.margin = (top, right, bottom, left);
    }

    pub fn get_or_calc(&mut self, size: (u32, u32), screen: (u32, u32)) -> (f32, f32) {
        if let Some(ref cache) = self.cache {
            if let Some(position) = cache.resolve(size, screen) {
                return position;
            }
        }

        let final_position = self.calc_position(size, screen);
        self.cache = Some(LayoutCache {
            size,
            screen,
            final_position,
        });
        final_position
    }

    fn calc_position(&self, size: (u32, u32), screen: (u32, u32)) -> (f32, f32) {
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

        (x, y)
    }
}

impl Default for OverlayLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct LayoutCache {
    pub size: (u32, u32),
    pub screen: (u32, u32),
    pub final_position: (f32, f32),
}

impl LayoutCache {
    pub fn resolve(&self, size: (u32, u32), screen: (u32, u32)) -> Option<(f32, f32)> {
        if size == self.size && self.screen == screen {
            Some(self.final_position)
        } else {
            None
        }
    }
}
