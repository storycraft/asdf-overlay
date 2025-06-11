use core::ffi::c_void;
use windows::{
    Win32::Graphics::{Direct3D::ID3DDestructionNotifier, Dxgi::IDXGISwapChain1},
    core::Interface,
};

pub fn register_swapchain_destruction_callback<F: FnOnce(&IDXGISwapChain1)>(
    swapchain: &IDXGISwapChain1,
    f: F,
) {
    struct Data<F> {
        swapchain: *mut c_void,
        f: F,
    }

    #[tracing::instrument]
    extern "system" fn callback<F: FnOnce(&IDXGISwapChain1)>(this: *mut c_void) {
        let this = unsafe { Box::from_raw(this.cast::<Data<F>>()) };
        let swapchain = unsafe { IDXGISwapChain1::from_raw_borrowed(&this.swapchain).unwrap() };
        (this.f)(swapchain)
    }

    let notifier = swapchain.cast::<ID3DDestructionNotifier>().unwrap();
    unsafe {
        // register with swapchain pointer without increasing ref
        notifier
            .RegisterDestructionCallback(
                Some(callback::<F>),
                Box::leak(Box::new(Data {
                    swapchain: swapchain.as_raw(),
                    f,
                })) as *mut _ as _,
            )
            .unwrap();
    }
}
