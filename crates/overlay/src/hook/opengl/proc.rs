use core::mem;

use asdf_overlay_event::{Event, SurfaceEvent};
use once_cell::sync::Lazy;
use scopeguard::defer;
use tracing::{Level, trace};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        self as msg, CallWindowProcA, GWLP_WNDPROC, SetWindowLongPtrA, WNDPROC,
    },
};

use crate::{
    event_sink::OverlayEventSink, surface::Surfaces, types::IntDashMap, util::get_client_size,
};

// HWND -> last WNDPROC
static MAP: Lazy<IntDashMap<u32, WNDPROC>> = Lazy::new(IntDashMap::default);

pub fn install(hwnd: HWND) {
    let key = hwnd.0 as u32;
    if MAP.contains_key(&key) {
        return;
    }

    MAP.entry(key).or_insert_with(|| unsafe {
        mem::transmute::<isize, WNDPROC>(SetWindowLongPtrA(
            hwnd,
            GWLP_WNDPROC,
            ogl_wnd_proc as *const () as _,
        ) as _)
    });
}

#[inline(always)]
fn proc(id: u64, msg: u32, lparam: LPARAM) {
    let msg::WM_WINDOWPOSCHANGED = msg else {
        return;
    };

    let winpos = unsafe { &*(lparam.0 as *const msg::WINDOWPOS) };
    if winpos.flags.0 & msg::SWP_NOSIZE.0 != 0 {
        return;
    }

    let (width, height) = get_client_size(HWND(id as _)).unwrap();
    Surfaces::state(id, |state| {
        state.resize(width, height);
        OverlayEventSink::emit(Event::Surface {
            id,
            event: SurfaceEvent::Resized { width, height },
        });
    });
}

#[tracing::instrument(level = Level::TRACE)]
unsafe extern "system" fn ogl_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc opengl hook called");

    let key = hwnd.0 as u32;
    defer!({
        // cleanup map
        if msg == msg::WM_NCDESTROY {
            trace!("cleanup ogl proc hook: {:?}", hwnd);
            MAP.remove(&key);
        }
    });
    let last_wnd_proc = *MAP.get(&key).unwrap();

    proc(key as _, msg, lparam);
    unsafe { CallWindowProcA(last_wnd_proc, hwnd, msg, wparam, lparam) }
}
