#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CopyRect {
    pub dst_x: u32,
    pub dst_y: u32,
    pub src: Rect,
}
