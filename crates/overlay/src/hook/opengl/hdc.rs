use scopeguard::defer;
use windows::Win32::{
    Foundation::{LUID, NTSTATUS},
    Graphics::Gdi::HDC,
};

#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
struct D3DKMT_OPENADAPTERFROMHDC {
    pub hdc: HDC,
    pub h_adapter: u32,
    pub adapter_luid: LUID,
    pub vid_pn_source_id: u32,
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct D3DKMT_CLOSEADAPTER {
    pub h_adapter: u32,
}

#[cfg_attr(
    not(target_arch = "x86"),
    link(name = "gdi32.dll", kind = "raw-dylib", modifiers = "+verbatim")
)]
#[cfg_attr(
    target_arch = "x86",
    link(
        name = "gdi32.dll",
        kind = "raw-dylib",
        modifiers = "+verbatim",
        import_name_type = "undecorated"
    )
)]
unsafe extern "system" {
    fn D3DKMTOpenAdapterFromHdc(param0: *mut D3DKMT_OPENADAPTERFROMHDC) -> NTSTATUS;
    fn D3DKMTCloseAdapter(param0: *const D3DKMT_CLOSEADAPTER) -> NTSTATUS;
}

pub fn get_hdc_adapter_luid(hdc: HDC) -> Option<LUID> {
    let mut open = D3DKMT_OPENADAPTERFROMHDC {
        hdc,
        ..Default::default()
    };

    unsafe { D3DKMTOpenAdapterFromHdc(&mut open) }.ok().ok()?;
    defer!(unsafe {
        _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
            h_adapter: open.h_adapter,
        });
    });

    Some(open.adapter_luid)
}
