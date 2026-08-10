/// Additional cursor resoucres for overlay.
///
/// See `../resources/cursors.rc`.
use asdf_overlay_common::cursor::Cursor;
use windows::{
    Win32::{
        Foundation::HINSTANCE,
        UI::WindowsAndMessaging::{
            HCURSOR, IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP, IDC_IBEAM, IDC_NO,
            IDC_PERSON, IDC_PIN, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
            IDC_UPARROW, IDC_WAIT, LoadCursorW,
        },
    },
    core::PCWSTR,
};

/// Load [`HCURSOR`] resource from `cursor` if exists.
pub(crate) fn load(hinstance: usize, cursor: Cursor) -> Option<HCURSOR> {
    #[inline]
    fn system_cursor(res: PCWSTR) -> Option<HCURSOR> {
        unsafe { LoadCursorW(None, res) }.ok()
    }

    #[inline]
    fn instance_cursor(hinstance: usize, res: PCWSTR) -> Option<HCURSOR> {
        unsafe { LoadCursorW(Some(HINSTANCE(hinstance as _)), res) }.ok()
    }

    match cursor {
        Cursor::Default => system_cursor(IDC_ARROW),
        Cursor::Help => system_cursor(IDC_HELP),
        Cursor::Pointer => system_cursor(IDC_HAND),
        Cursor::Progress => system_cursor(IDC_APPSTARTING),
        Cursor::Wait => system_cursor(IDC_WAIT),
        Cursor::Cell => instance_cursor(hinstance, IDC_CELL),
        Cursor::Crosshair => system_cursor(IDC_CROSS),
        Cursor::Text => system_cursor(IDC_IBEAM),
        Cursor::VerticalText => instance_cursor(hinstance, IDC_VERTICALTEXT),
        Cursor::Alias => instance_cursor(hinstance, IDC_ALIAS),
        Cursor::Copy => instance_cursor(hinstance, IDC_COPYCUR),
        Cursor::Move => system_cursor(IDC_SIZEALL),
        Cursor::NotAllowed => system_cursor(IDC_NO),
        Cursor::Grab => instance_cursor(hinstance, IDC_HAND_GRAB),
        Cursor::Grabbing => instance_cursor(hinstance, IDC_HAND_GRABBING),
        Cursor::ColResize => instance_cursor(hinstance, IDC_COLRESIZE),
        Cursor::RowResize => instance_cursor(hinstance, IDC_ROWRESIZE),
        Cursor::EastWestResize => system_cursor(IDC_SIZEWE),
        Cursor::NorthSouthResize => system_cursor(IDC_SIZENS),
        Cursor::NorthEastSouthWestResize => system_cursor(IDC_SIZENESW),
        Cursor::NorthWestSouthEastResize => system_cursor(IDC_SIZENWSE),
        Cursor::ZoomIn => instance_cursor(hinstance, IDC_ZOOMIN),
        Cursor::ZoomOut => instance_cursor(hinstance, IDC_ZOOMOUT),
        Cursor::UpArrow => system_cursor(IDC_UPARROW),
        Cursor::Pin => system_cursor(IDC_PIN),
        Cursor::Person => system_cursor(IDC_PERSON),
        Cursor::Pen => system_cursor(PCWSTR(32631 as _)), // https://learn.microsoft.com/en-us/windows/win32/menurc/about-cursors
        Cursor::Cd => system_cursor(PCWSTR(32663 as _)),
        Cursor::PanMiddle => instance_cursor(hinstance, IDC_PAN_MIDDLE),
        Cursor::PanMiddleHorizontal => instance_cursor(hinstance, IDC_PAN_MIDDLE_HORIZONTAL),
        Cursor::PanMiddleVertical => instance_cursor(hinstance, IDC_PAN_MIDDLE_VERTICAL),
        Cursor::PanEast => instance_cursor(hinstance, IDC_PAN_EAST),
        Cursor::PanNorth => instance_cursor(hinstance, IDC_PAN_NORTH),
        Cursor::PanNorthEast => instance_cursor(hinstance, IDC_PAN_NORTH_EAST),
        Cursor::PanNorthWest => instance_cursor(hinstance, IDC_PAN_NORTH_WEST),
        Cursor::PanSouth => instance_cursor(hinstance, IDC_PAN_SOUTH),
        Cursor::PanSouthEast => instance_cursor(hinstance, IDC_PAN_SOUTH_EAST),
        Cursor::PanSouthWest => instance_cursor(hinstance, IDC_PAN_SOUTH_WEST),
        Cursor::PanWest => instance_cursor(hinstance, IDC_PAN_WEST),
    }
}

const IDC_ALIAS: PCWSTR = PCWSTR(1 as _);
const IDC_CELL: PCWSTR = PCWSTR(2 as _);
const IDC_COLRESIZE: PCWSTR = PCWSTR(3 as _);
const IDC_COPYCUR: PCWSTR = PCWSTR(4 as _);
const IDC_HAND_GRAB: PCWSTR = PCWSTR(5 as _);
const IDC_HAND_GRABBING: PCWSTR = PCWSTR(6 as _);
const IDC_PAN_EAST: PCWSTR = PCWSTR(7 as _);
const IDC_PAN_MIDDLE: PCWSTR = PCWSTR(8 as _);
const IDC_PAN_MIDDLE_HORIZONTAL: PCWSTR = PCWSTR(9 as _);
const IDC_PAN_MIDDLE_VERTICAL: PCWSTR = PCWSTR(10 as _);
const IDC_PAN_NORTH: PCWSTR = PCWSTR(11 as _);
const IDC_PAN_NORTH_EAST: PCWSTR = PCWSTR(12 as _);
const IDC_PAN_NORTH_WEST: PCWSTR = PCWSTR(13 as _);
const IDC_PAN_SOUTH: PCWSTR = PCWSTR(14 as _);
const IDC_PAN_SOUTH_EAST: PCWSTR = PCWSTR(15 as _);
const IDC_PAN_SOUTH_WEST: PCWSTR = PCWSTR(16 as _);
const IDC_PAN_WEST: PCWSTR = PCWSTR(17 as _);
const IDC_ROWRESIZE: PCWSTR = PCWSTR(18 as _);
const IDC_VERTICALTEXT: PCWSTR = PCWSTR(19 as _);
const IDC_ZOOMIN: PCWSTR = PCWSTR(20 as _);
const IDC_ZOOMOUT: PCWSTR = PCWSTR(21 as _);
