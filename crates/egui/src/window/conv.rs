use asdf_overlay_window_event::input::Key;

pub fn conv_key(key: Key) -> Option<egui::Key> {
    Some(match key.code.get() {
        8 => egui::Key::Backspace,
        9 => egui::Key::Tab,
        13 => egui::Key::Enter,
        16 => {
            if key.extended {
                egui::Key::ShiftRight
            } else {
                egui::Key::ShiftLeft
            }
        }
        17 => {
            if key.extended {
                egui::Key::ControlRight
            } else {
                egui::Key::ControlLeft
            }
        }
        18 => {
            if key.extended {
                egui::Key::AltRight
            } else {
                egui::Key::AltLeft
            }
        }
        27 => egui::Key::Escape,
        32 => egui::Key::Space,
        33 => egui::Key::PageUp,
        34 => egui::Key::PageDown,
        35 => egui::Key::End,
        36 => egui::Key::Home,
        37 => egui::Key::ArrowLeft,
        38 => egui::Key::ArrowUp,
        39 => egui::Key::ArrowRight,
        45 => egui::Key::Insert,
        46 => egui::Key::Delete,
        48 => egui::Key::Num0,
        49 => egui::Key::Num1,
        50 => egui::Key::Num2,
        51 => egui::Key::Num3,
        52 => egui::Key::Num4,
        53 => egui::Key::Num5,
        54 => egui::Key::Num6,
        55 => egui::Key::Num7,
        56 => egui::Key::Num8,
        57 => egui::Key::Num9,
        65 => egui::Key::A,
        66 => egui::Key::B,
        67 => egui::Key::C,
        68 => egui::Key::D,
        69 => egui::Key::E,
        70 => egui::Key::F,
        71 => egui::Key::G,
        72 => egui::Key::H,
        73 => egui::Key::I,
        74 => egui::Key::J,
        75 => egui::Key::K,
        76 => egui::Key::L,
        77 => egui::Key::M,
        78 => egui::Key::N,
        79 => egui::Key::O,
        80 => egui::Key::P,
        81 => egui::Key::Q,
        82 => egui::Key::R,
        83 => egui::Key::S,
        84 => egui::Key::T,
        85 => egui::Key::U,
        86 => egui::Key::V,
        87 => egui::Key::W,
        88 => egui::Key::X,
        89 => egui::Key::Y,
        90 => egui::Key::Z,
        91 => egui::Key::SuperLeft,
        92 => egui::Key::SuperRight,
        96 => egui::Key::Num0,
        97 => egui::Key::Num1,
        98 => egui::Key::Num2,
        99 => egui::Key::Num3,
        100 => egui::Key::Num4,
        101 => egui::Key::Num5,
        102 => egui::Key::Num6,
        103 => egui::Key::Num7,
        104 => egui::Key::Num8,
        105 => egui::Key::Num9,
        106 => egui::Key::Delete,
        108 => egui::Key::Minus,
        109 => egui::Key::Minus,
        110 => egui::Key::Period,
        111 => egui::Key::Slash,
        112 => egui::Key::F1,
        113 => egui::Key::F2,
        114 => egui::Key::F3,
        115 => egui::Key::F4,
        116 => egui::Key::F5,
        117 => egui::Key::F6,
        118 => egui::Key::F7,
        119 => egui::Key::F8,
        120 => egui::Key::F9,
        121 => egui::Key::F10,
        122 => egui::Key::F11,
        123 => egui::Key::F12,
        124 => egui::Key::F13,
        125 => egui::Key::F14,
        126 => egui::Key::F15,
        127 => egui::Key::F16,
        128 => egui::Key::F17,
        129 => egui::Key::F18,
        130 => egui::Key::F19,
        131 => egui::Key::F20,
        132 => egui::Key::F21,
        133 => egui::Key::F22,
        134 => egui::Key::F23,
        135 => egui::Key::F24,
        186 => egui::Key::Semicolon,
        187 => egui::Key::Equals,
        188 => egui::Key::Comma,
        189 => egui::Key::Minus,
        190 => egui::Key::Period,
        191 => egui::Key::Slash,
        192 => egui::Key::Backtick,
        219 => egui::Key::OpenCurlyBracket,
        220 => egui::Key::Backslash,
        221 => egui::Key::CloseCurlyBracket,
        222 => egui::Key::Quote,
        225 => egui::Key::AltRight,

        // Unknown key code
        _ => return None,
    })
}
