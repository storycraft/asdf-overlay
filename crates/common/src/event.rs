use bincode::{Decode, Encode};

#[derive(Debug, Encode, Decode, Clone)]
pub enum ClientEvent {
    Window { hwnd: u32, event: WindowEvent },
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum WindowEvent {
    Added,
    Resized { width: u32, height: u32 },
    Destroyed,
}
