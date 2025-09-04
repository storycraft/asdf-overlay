use scopeguard::defer;
use windows::{
    Wdk::Graphics::Direct3D::{
        D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMHDC, D3DKMTCloseAdapter,
        D3DKMTOpenAdapterFromHdc,
    },
    Win32::{Foundation::LUID, Graphics::Gdi::HDC},
};

pub fn get_hdc_adapter_luid(hdc: HDC) -> Option<LUID> {
    let mut open = D3DKMT_OPENADAPTERFROMHDC {
        hDc: hdc,
        ..Default::default()
    };

    unsafe { D3DKMTOpenAdapterFromHdc(&mut open) }.ok().ok()?;
    defer!(unsafe {
        _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
            hAdapter: open.hAdapter,
        });
    });

    Some(open.AdapterLuid)
}
