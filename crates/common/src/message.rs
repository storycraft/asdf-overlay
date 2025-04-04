use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode)]
pub enum Request {
    /// Change draw position
    Position(UpdatePosition),
    /// Change image
    Texture(UpdateTexture),
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
pub struct UpdateTexture {
    pub width: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum Response {
    Success,
    Failed { message: String },
}
