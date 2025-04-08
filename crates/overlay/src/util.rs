use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::GetClientRect,
};

pub fn get_client_size(win: HWND) -> anyhow::Result<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(win, &mut rect)? };

    Ok((
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ))
}
