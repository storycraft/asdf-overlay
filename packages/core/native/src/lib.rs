mod conv;
mod overlay;
mod surface;
mod util;

use mimalloc::MiMalloc;
use neon::prelude::*;
use rustc_hash::FxBuildHasher;

type FxSccMap<K, V> = scc::HashMap<K, V, FxBuildHasher>;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    surface::export_module_functions(&mut cx)?;
    overlay::export_module_functions(&mut cx)?;
    Ok(())
}
