use core::mem;

use windows::Win32::Graphics::Direct3D9::{D3DSPD_IUNKNOWN, IDirect3DResource9};
use windows_core::{GUID, IUnknown, Interface as _, implement, interface};

pub fn register_destruction_callback(
    resource: &IDirect3DResource9,
    f: impl FnOnce() + Send + Sync + 'static,
) -> anyhow::Result<GUID> {
    let guid = GUID::new()?;
    let notifier: IUnknown = Notifier { inner: Some(f) }.into();

    unsafe {
        resource.SetPrivateData(
            &guid,
            notifier.as_raw() as _,
            mem::size_of::<IUnknown>() as _,
            D3DSPD_IUNKNOWN as _,
        )?;
    };
    Ok(guid)
}

#[interface("f4b181cd-9dc0-44f6-b9b4-d5226dfc394d")]
unsafe trait INotifier: windows::core::IUnknown {}

#[implement(INotifier)]
struct Notifier<F>
where
    F: FnOnce() + Send + Sync + 'static,
{
    inner: Option<F>,
}

impl<F> INotifier_Impl for Notifier_Impl<F> where F: FnOnce() + Send + Sync + 'static {}

impl<F> Drop for Notifier<F>
where
    F: FnOnce() + Send + Sync + 'static,
{
    fn drop(&mut self) {
        let Some(f) = self.inner.take() else {
            return;
        };

        f()
    }
}
