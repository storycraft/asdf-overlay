use windows::Win32::UI::WindowsAndMessaging::{HCURSOR, IDC_ARROW, LoadCursorW};

pub fn default_cursor() -> HCURSOR {
    unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap()
}
