use core::ffi::c_void;
use windows::{
    Win32::Graphics::{Direct3D::ID3DDestructionNotifier, Dxgi::IDXGISwapChain1},
    core::Interface,
};

pub fn register_swapchain_destruction_callback<F: FnOnce(usize) + Send + 'static>(
    swapchain: &IDXGISwapChain1,
    f: F,
) {
    struct Data<F> {
        this: usize,
        f: F,
    }

    #[tracing::instrument]
    extern "system" fn callback<F: FnOnce(usize)>(this: *mut c_void) {
        let this = unsafe { Box::from_raw(this.cast::<Data<F>>()) };
        (this.f)(this.this)
    }

    let notifier = swapchain.cast::<ID3DDestructionNotifier>().unwrap();
    unsafe {
        // register with swapchain pointer without increasing ref
        notifier
            .RegisterDestructionCallback(
                Some(callback::<F>),
                Box::leak(Box::new(Data {
                    this: swapchain.as_raw() as _,
                    f,
                })) as *mut _ as _,
            )
            .unwrap();
    }
}
