use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::GetClientRect,
};

/// Get client area size of the window.
pub fn get_client_size(hwnd: HWND) -> anyhow::Result<(u32, u32)> {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)?;
        Ok((rect.right as u32, rect.bottom as u32))
    }
}
