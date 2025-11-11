mod conv;
mod overlay;
mod surface;
mod util;

use mimalloc::MiMalloc;
use neon::prelude::*;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    surface::export_module_functions(&mut cx)?;
    overlay::export_module_functions(&mut cx)?;
    Ok(())
}
