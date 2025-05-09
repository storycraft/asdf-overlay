pub mod input;

use bincode::{Decode, Encode};
use input::InputEvent;

#[derive(Debug, Encode, Decode, Clone)]
pub enum ClientEvent {
    Window { hwnd: u32, event: WindowEvent },
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum WindowEvent {
    Added,
    Resized { width: u32, height: u32 },
    Input(InputEvent),
    Destroyed,
}
