pub mod input;

use bincode::{Decode, Encode};
use input::InputEvent;

#[derive(Debug, Encode, Decode, Clone)]
pub enum ClientEvent {
    Window { hwnd: u32, event: WindowEvent },
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum WindowEvent {
    // Window is hooked and added
    Added,

    // Window is resized
    Resized { width: u32, height: u32 },

    // Captured window input
    Input(InputEvent),

    // Input capture started by user
    InputCaptureStart,

    // Input capture ended by user
    InputCaptureEnd,

    // Window destroyed
    Destroyed,
}
