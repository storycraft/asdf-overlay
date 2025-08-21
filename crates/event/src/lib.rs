pub mod input;

use input::InputEvent;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum ClientEvent {
    Window { id: u32, event: WindowEvent },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub enum WindowEvent {
    // Window is hooked and added
    Added {
        width: u32,
        height: u32,
        gpu_id: GpuLuid,
    },

    // Window is resized
    Resized {
        width: u32,
        height: u32,
    },

    // Captured window input
    Input(InputEvent),

    // Input blocking ended
    InputBlockingEnded,

    // Window destroyed
    Destroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct GpuLuid {
    pub low: u32,
    pub high: i32,
}
