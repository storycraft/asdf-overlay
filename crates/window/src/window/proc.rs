use core::{alloc::Layout, mem, slice};
use scopeguard::defer;
use std::alloc;
use tracing::trace;
use utf16string::{LittleEndian, WStr, WString};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Globalization::LCIDToLocaleName,
    System::SystemServices::{LOCALE_NAME_MAX_LENGTH, SORT_DEFAULT},
    UI::{
        Input::{
            Ime::{
                self as ime, CANDIDATELIST, HIMC, IME_COMPOSITION_STRING, IME_CONVERSION_MODE,
                ImmGetCandidateListW, ImmGetCompositionStringW, ImmGetContext,
                ImmGetConversionStatus, ImmReleaseContext,
            },
            KeyboardAndMouse::GetKeyboardLayout,
        },
        WindowsAndMessaging::{
            self as msg, CallWindowProcA, DefWindowProcA, SetCursor, WM_NCDESTROY,
        },
    },
};

use crate::{
    Backends, cursors,
    event::{
        BackendEvent, WindowEvent,
        input::{ConversionMode, Ime, ImeCandidateList, InputEvent, KeyboardInput},
    },
    window::{ImeState, ListenInputFlags, get_client_size},
};

#[tracing::instrument]
pub(super) unsafe extern "system" fn hooked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    trace!("WndProc called");

    defer!({
        // cleanup backend
        if msg == WM_NCDESTROY {
            trace!("cleanup hwnd: {:?}", hwnd);
            Backends::get().cleanup_window(hwnd.0 as u32);
        }
    });

    if let Some(ret) = process_wnd_proc(hwnd.0 as u32, msg, wparam, lparam) {
        return ret;
    }

    let original_proc = Backends::get().window_state(hwnd.0 as u32, |state| state.original_proc);
    unsafe { CallWindowProcA(original_proc, hwnd, msg, wparam, lparam) }
}

#[inline]
fn process_wnd_proc(hwnd: u32, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        msg::WM_WINDOWPOSCHANGED => {
            let (width, height) = get_client_size(HWND(hwnd as _)).unwrap();

            Backends::get().window_state(hwnd, |state| {
                if state.size() != (width, height) {
                    state.set_size(width, height);

                    Backends::get().emit(BackendEvent::Window {
                        id: hwnd,
                        event: WindowEvent::Resized { width, height },
                    });
                }
            });
        }

        // set cursor in client area
        msg::WM_SETCURSOR
            if {
                let [area, _] = bytemuck::cast::<_, [u16; 2]>(lparam.0 as u32);
                // check if cursor is on client
                area == 1
            } =>
        {
            let global_state = Backends::get();
            if global_state.input_blocked() {
                unsafe { SetCursor(global_state.blocking_cursor.read().and_then(cursors::load)) };
                return Some(LRESULT(1));
            }
        }

        // stop input capture when user request to
        msg::WM_CLOSE => {
            let global_state = Backends::get();
            if global_state.input_blocked() {
                global_state.unblock_input();
                return Some(LRESULT(0));
            }
        }

        msg::WM_APPCOMMAND if Backends::get().input_blocked() => {
            return Some(unsafe { DefWindowProcA(HWND(hwnd as _), msg, wparam, lparam) });
        }

        // block other keyboard, mouse event
        msg::WM_CAPTURECHANGED
        | msg::WM_ACTIVATE
        | msg::WM_ACTIVATEAPP
        | msg::WM_SETFOCUS
        | msg::WM_KILLFOCUS
        | msg::WM_DEADCHAR
        | msg::WM_HOTKEY
        | msg::WM_SYSDEADCHAR
        | msg::WM_UNICHAR
        | msg::WM_IME_REQUEST
            if Backends::get().input_blocked() =>
        {
            return Some(LRESULT(0));
        }

        msg::WM_INPUTLANGCHANGEREQUEST if Backends::get().input_blocked() => {
            return Some(unsafe { DefWindowProcA(HWND(hwnd as _), msg, wparam, lparam) });
        }

        msg::WM_IME_NOTIFY => {
            let listening_keyboard = Backends::get().window_state(hwnd, |state| {
                state.input_flags().contains(ListenInputFlags::KEYBOARD)
            });
            if !listening_keyboard {
                return None;
            }

            handle_ime_notify(hwnd, wparam.0 as _);
            if Backends::get().input_blocked() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_INPUTLANGCHANGE => {
            let listening_keyboard = Backends::get().window_state(hwnd, |state| {
                state.input_flags().contains(ListenInputFlags::KEYBOARD)
            });
            if !listening_keyboard {
                return None;
            }

            if let Some(lang) = get_lang_id_locale(lparam.0 as u16) {
                emit_ime_event(hwnd, Ime::Changed(lang));
            }

            if Backends::get().input_blocked() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_SETCONTEXT => {
            let listening_keyboard = Backends::get().window_state(hwnd, |state| {
                state.input_flags().contains(ListenInputFlags::KEYBOARD)
            });
            if !listening_keyboard {
                return None;
            }

            let lang_id = unsafe { GetKeyboardLayout(0) }.0 as u16;
            emit_ime_event(
                hwnd,
                if wparam.0 != 0 {
                    Ime::Enabled {
                        lang: get_lang_id_locale(lang_id).unwrap_or_else(|| "en".to_string()),
                        conversion: with_himc(hwnd, ime_conversion_mode),
                    }
                } else {
                    Ime::Disabled
                },
            );

            if Backends::get().input_blocked() {
                return Some(unsafe {
                    DefWindowProcA(
                        HWND(hwnd as _),
                        msg,
                        wparam,
                        // Disable composition, candinate window
                        LPARAM(0),
                    )
                });
            }
        }

        msg::WM_IME_STARTCOMPOSITION => {
            Backends::get().window_state(hwnd, |state| {
                *state.ime.write() = ImeState::Enabled;
            });

            if Backends::get().input_blocked() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_COMPOSITION => {
            let (listening_keyboard, ime) = Backends::get().window_state(hwnd, |state| {
                (
                    state.input_flags().contains(ListenInputFlags::KEYBOARD),
                    *state.ime.read(),
                )
            });
            if !listening_keyboard {
                return None;
            }

            if ime != ImeState::Disabled {
                with_himc(hwnd, |himc| {
                    let comp = IME_COMPOSITION_STRING(lparam.0 as _);

                    // cancelled
                    if comp == IME_COMPOSITION_STRING(0) {
                        emit_ime_event(hwnd, Ime::Commit(String::new()));
                    }

                    if comp.contains(ime::GCS_RESULTSTR)
                        && let Some(text) = get_ime_string(himc, ime::GCS_RESULTSTR)
                    {
                        Backends::get()
                            .window_state(hwnd, |proc| *proc.ime.write() = ImeState::Enabled);
                        emit_ime_event(hwnd, Ime::Commit(text.to_utf8()));
                    }

                    if comp.0 & (ime::GCS_COMPSTR | ime::GCS_COMPATTR | ime::GCS_CURSORPOS).0 != 0 {
                        let caret = if !comp.contains(IME_COMPOSITION_STRING(ime::CS_NOMOVECARET))
                            && comp.contains(ime::GCS_CURSORPOS)
                        {
                            unsafe {
                                ImmGetCompositionStringW(himc, ime::GCS_CURSORPOS, None, 0) as usize
                            }
                        } else {
                            0
                        };

                        if let Some(text) = get_ime_string(himc, ime::GCS_COMPSTR) {
                            Backends::get()
                                .window_state(hwnd, |proc| *proc.ime.write() = ImeState::Compose);

                            emit_ime_event(
                                hwnd,
                                Ime::Compose {
                                    text: text.to_utf8(),
                                    caret,
                                },
                            );
                        }
                    }
                });
            }

            if Backends::get().input_blocked() {
                return Some(LRESULT(0));
            }
        }

        msg::WM_IME_ENDCOMPOSITION => {
            let ime = Backends::get().window_state(hwnd, |state| {
                mem::replace(&mut *state.ime.write(), ImeState::Disabled)
            });

            if ime == ImeState::Compose {
                let himc = unsafe { ImmGetContext(HWND(hwnd as _)) };
                defer!(unsafe {
                    _ = ImmReleaseContext(HWND(hwnd as _), himc);
                });

                if let Some(text) = get_ime_string(himc, ime::GCS_RESULTSTR) {
                    emit_ime_event(hwnd, Ime::Commit(text.to_utf8()));
                }
            }

            if Backends::get().input_blocked() {
                return Some(LRESULT(0));
            }
        }

        _ => {}
    }
    None
}

fn emit_ime_event(hwnd: u32, ime: Ime) {
    Backends::get().emit(BackendEvent::Window {
        id: hwnd,
        event: WindowEvent::Input(InputEvent::Keyboard(KeyboardInput::Ime(ime))),
    });
}

fn handle_ime_notify(hwnd: u32, command: u32) {
    match command {
        ime::IMN_SETCONVERSIONMODE => emit_ime_event(
            hwnd,
            Ime::ConversionChanged(with_himc(hwnd, ime_conversion_mode)),
        ),

        ime::IMN_OPENCANDIDATE | ime::IMN_CHANGECANDIDATE => {
            with_himc(hwnd, |himc| {
                if let Some(candidate_list) = get_ime_candidate_list(himc, 0) {
                    emit_ime_event(hwnd, Ime::CandidateChanged(candidate_list));
                }
            });
        }

        ime::IMN_CLOSECANDIDATE => emit_ime_event(hwnd, Ime::CandidateClosed),

        _ => {}
    }
}

#[inline]
fn with_himc<R>(hwnd: u32, f: impl FnOnce(HIMC) -> R) -> R {
    let hwnd = HWND(hwnd as _);
    let himc = unsafe { ImmGetContext(hwnd) };
    defer!(unsafe {
        _ = ImmReleaseContext(hwnd, himc);
    });

    f(himc)
}

fn get_ime_string(himc: HIMC, comp: IME_COMPOSITION_STRING) -> Option<WString<LittleEndian>> {
    let byte_size = unsafe { ImmGetCompositionStringW(himc, comp, None, 0) };
    if byte_size >= 0 {
        let mut buf = vec![0_u8; byte_size as usize];

        unsafe {
            ImmGetCompositionStringW(himc, comp, Some(buf.as_mut_ptr().cast()), buf.len() as _)
        };

        WString::from_utf16le(buf).ok()
    } else {
        None
    }
}

fn get_ime_candidate_list(himc: HIMC, index: u32) -> Option<ImeCandidateList> {
    let byte_size = unsafe { ImmGetCandidateListW(himc, index, None, 0) };
    if byte_size == 0 {
        return None;
    }

    let layout = Layout::from_size_align(byte_size as _, mem::align_of::<CANDIDATELIST>()).ok()?;
    let mut candidate_list_ptr = scopeguard::guard(
        unsafe { alloc::alloc(layout) }.cast::<CANDIDATELIST>(),
        |ptr| unsafe {
            alloc::dealloc(ptr as _, layout);
        },
    );

    let res = unsafe { ImmGetCandidateListW(himc, index, Some(*candidate_list_ptr), byte_size) };
    if res == 0 {
        return None;
    }

    let CANDIDATELIST {
        dwCount: count,
        dwSelection: selected_index,
        dwPageStart: page_start_index,
        dwPageSize: page_size,
        ..
    } = unsafe { **candidate_list_ptr };
    let candidates = {
        let mut list = Vec::with_capacity(count as _);
        let base = unsafe { &raw mut (**candidate_list_ptr).dwOffset }.cast::<u32>();
        for i in 0..count {
            let candidate_offset = unsafe { *base.add(i as _) };
            let candidate_start = unsafe {
                candidate_list_ptr
                    .byte_add(candidate_offset as _)
                    .cast::<u16>()
            };
            let size = {
                let mut len = 0;
                while (unsafe { *candidate_start.add(len) }) != 0 {
                    len += 1;
                }
                len * 2
            };

            list.push(
                unsafe {
                    WStr::from_utf16le_unchecked(slice::from_raw_parts(
                        candidate_start.cast::<u8>(),
                        size,
                    ))
                }
                .to_utf8(),
            );
        }
        list
    };

    Some(ImeCandidateList {
        page_start_index,
        page_size,
        selected_index,
        candidates,
    })
}

fn get_lang_id_locale(lang_id: u16) -> Option<String> {
    let lcid = const { SORT_DEFAULT << 16 } | lang_id as u32;

    let mut buf = [0_u16; LOCALE_NAME_MAX_LENGTH as usize];
    let size = unsafe { LCIDToLocaleName(lcid, Some(&mut buf), 0) };
    if size > 0 {
        Some(
            WStr::from_utf16le(bytemuck::cast_slice::<_, u8>(&buf[..(size - 1) as usize]))
                .ok()?
                .to_utf8(),
        )
    } else {
        None
    }
}

fn ime_conversion_mode(himc: HIMC) -> ConversionMode {
    let mut raw_mode = IME_CONVERSION_MODE(0);
    _ = unsafe { ImmGetConversionStatus(himc, Some(&mut raw_mode), None) };

    let mut mode = ConversionMode::empty();
    if raw_mode.contains(ime::IME_CMODE_NATIVE) {
        mode |= ConversionMode::NATIVE;
    }
    if raw_mode.contains(ime::IME_CMODE_FULLSHAPE) {
        mode |= ConversionMode::FULLSHAPE;
    }
    if raw_mode.contains(ime::IME_CMODE_NOCONVERSION) {
        mode |= ConversionMode::NO_CONVERSION;
    }
    if raw_mode.contains(ime::IME_CMODE_HANJACONVERT) {
        mode |= ConversionMode::HANJA_CONVERT;
    }
    if raw_mode.contains(ime::IME_CMODE_KATAKANA) {
        mode |= ConversionMode::KATAKANA;
    }
    mode
}
