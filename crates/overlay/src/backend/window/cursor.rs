use asdf_overlay_common::cursor::Cursor;
use windows::{
    Win32::UI::WindowsAndMessaging::{
        HCURSOR, IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP, IDC_IBEAM, IDC_NO,
        IDC_PERSON, IDC_PIN, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
        IDC_UPARROW, IDC_WAIT, LoadCursorW,
    },
    core::PCWSTR,
};

use crate::{
    instance,
    resources::cursors::{
        IDC_ALIAS, IDC_CELL, IDC_COLRESIZE, IDC_COPYCUR, IDC_HAND_GRAB, IDC_HAND_GRABBING,
        IDC_PAN_EAST, IDC_PAN_MIDDLE, IDC_PAN_MIDDLE_HORIZONTAL, IDC_PAN_MIDDLE_VERTICAL,
        IDC_PAN_NORTH, IDC_PAN_NORTH_EAST, IDC_PAN_NORTH_WEST, IDC_PAN_SOUTH, IDC_PAN_SOUTH_EAST,
        IDC_PAN_SOUTH_WEST, IDC_PAN_WEST, IDC_ROWRESIZE, IDC_VERTICALTEXT, IDC_ZOOMIN, IDC_ZOOMOUT,
    },
};

pub fn load_cursor(cursor: Cursor) -> Option<HCURSOR> {
    #[inline]
    fn system_cursor(res: PCWSTR) -> Option<HCURSOR> {
        unsafe { LoadCursorW(None, res) }.ok()
    }

    #[inline]
    fn instance_cursor(res: PCWSTR) -> Option<HCURSOR> {
        unsafe { LoadCursorW(Some(instance()), res) }.ok()
    }

    match cursor {
        Cursor::Default => system_cursor(IDC_ARROW),
        Cursor::Help => system_cursor(IDC_HELP),
        Cursor::Pointer => system_cursor(IDC_HAND),
        Cursor::Progress => system_cursor(IDC_APPSTARTING),
        Cursor::Wait => system_cursor(IDC_WAIT),
        Cursor::Cell => instance_cursor(IDC_CELL),
        Cursor::Crosshair => system_cursor(IDC_CROSS),
        Cursor::Text => system_cursor(IDC_IBEAM),
        Cursor::VerticalText => instance_cursor(IDC_VERTICALTEXT),
        Cursor::Alias => instance_cursor(IDC_ALIAS),
        Cursor::Copy => instance_cursor(IDC_COPYCUR),
        Cursor::Move => system_cursor(IDC_SIZEALL),
        Cursor::NotAllowed => system_cursor(IDC_NO),
        Cursor::Grab => instance_cursor(IDC_HAND_GRAB),
        Cursor::Grabbing => instance_cursor(IDC_HAND_GRABBING),
        Cursor::ColResize => instance_cursor(IDC_COLRESIZE),
        Cursor::RowResize => instance_cursor(IDC_ROWRESIZE),
        Cursor::EastWestResize => system_cursor(IDC_SIZEWE),
        Cursor::NorthSouthResize => system_cursor(IDC_SIZENS),
        Cursor::NorthEastSouthWestResize => system_cursor(IDC_SIZENESW),
        Cursor::NorthWestSouthEastResize => system_cursor(IDC_SIZENWSE),
        Cursor::ZoomIn => instance_cursor(IDC_ZOOMIN),
        Cursor::ZoomOut => instance_cursor(IDC_ZOOMOUT),
        Cursor::UpArrow => system_cursor(IDC_UPARROW),
        Cursor::Pin => system_cursor(IDC_PIN),
        Cursor::Person => system_cursor(IDC_PERSON),
        Cursor::Pen => system_cursor(PCWSTR(32631 as _)), // https://learn.microsoft.com/en-us/windows/win32/menurc/about-cursors
        Cursor::Cd => system_cursor(PCWSTR(32663 as _)),
        Cursor::PanMiddle => instance_cursor(IDC_PAN_MIDDLE),
        Cursor::PanMiddleHorizontal => instance_cursor(IDC_PAN_MIDDLE_HORIZONTAL),
        Cursor::PanMiddleVertical => instance_cursor(IDC_PAN_MIDDLE_VERTICAL),
        Cursor::PanEast => instance_cursor(IDC_PAN_EAST),
        Cursor::PanNorth => instance_cursor(IDC_PAN_NORTH),
        Cursor::PanNorthEast => instance_cursor(IDC_PAN_NORTH_EAST),
        Cursor::PanNorthWest => instance_cursor(IDC_PAN_NORTH_WEST),
        Cursor::PanSouth => instance_cursor(IDC_PAN_SOUTH),
        Cursor::PanSouthEast => instance_cursor(IDC_PAN_SOUTH_EAST),
        Cursor::PanSouthWest => instance_cursor(IDC_PAN_SOUTH_WEST),
        Cursor::PanWest => instance_cursor(IDC_PAN_WEST),
    }
}
