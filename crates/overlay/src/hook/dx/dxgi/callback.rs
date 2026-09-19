use core::ffi::c_void;
use tracing::Level;
use windows::{
    Win32::Graphics::{Direct3D::ID3DDestructionNotifier, Dxgi::IDXGISwapChain},
    core::Interface,
};

pub fn register_swapchain_destruction_callback<F: FnOnce() + Send + 'static>(
    swapchain: &IDXGISwapChain,
    f: F,
) {
    #[tracing::instrument(level = Level::DEBUG)]
    extern "system" fn callback<F: FnOnce()>(this: *mut c_void) {
        let this = unsafe { Box::from_raw(this.cast::<F>()) };
        this()
    }

    let notifier = swapchain.cast::<ID3DDestructionNotifier>().unwrap();
    unsafe {
        // register with swapchain pointer without increasing ref
        notifier
            .RegisterDestructionCallback(Some(callback::<F>), Box::leak(Box::new(f)) as *mut _ as _)
            .unwrap();
    }
}
