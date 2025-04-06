use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode)]
pub enum Request {
    /// Change draw position
    Position(UpdatePosition),
    /// Update overlay using bitmap
    Bitmap(UpdateBitmap),
    /// Close and exit overlay
    Close,
    Test,
}

#[derive(Debug, Encode, Decode)]
pub struct UpdatePosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Encode, Decode)]
pub struct UpdateBitmap {
    pub width: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum Response {
    Success,
    Failed { message: String },
}
