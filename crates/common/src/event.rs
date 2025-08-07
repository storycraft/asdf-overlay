pub mod input;

use bincode::{Decode, Encode};
use input::InputEvent;

#[derive(Debug, Encode, Decode, Clone)]
pub enum ClientEvent {
    Window { id: u32, event: WindowEvent },
}

#[derive(Debug, Encode, Decode, Clone)]
pub enum WindowEvent {
    // Window is hooked and added
    Added { width: u32, height: u32 },

    // Window is resized
    Resized { width: u32, height: u32 },

    // Captured window input
    Input(InputEvent),

    // Input blocking ended
    InputBlockingEnded,

    // Window destroyed
    Destroyed,
}
