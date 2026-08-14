pub mod event;
pub mod overlay;
pub mod surface;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
