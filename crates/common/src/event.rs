use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode, Clone)]
#[non_exhaustive]
pub enum ClientEvent {
    Window { hwnd: u32, event: WindowEvent },
}

#[derive(Debug, Encode, Decode, Clone)]
#[non_exhaustive]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
    Destroyed,
}
