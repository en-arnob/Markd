//! The welcome (start) screen shown when no document is open: a left pane with
//! the app icon/name and a right pane with the owner-drawn Open/Edit buttons.

use std::ptr::null_mut;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontIndirectW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, HBRUSH, HDC, HGDIOBJ, LOGFONTW, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED, ODT_BUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DrawIconEx, GetClientRect, GetWindowTextW, LoadImageW, MoveWindow, ShowWindow,
    BS_OWNERDRAW, DI_NORMAL, HICON, HMENU, IMAGE_ICON, LR_DEFAULTCOLOR, SW_HIDE, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_TABSTOP,
};

use crate::app::{
    state, APP_TITLE, ID_WELCOME_EDIT, ID_WELCOME_OPEN, VIEW_PADDING, WELCOME_BTN_GAP,
    WELCOME_BTN_H, WELCOME_BTN_W, WELCOME_ICON,
};
use crate::theme::current_dark;
use crate::util::{colorref, fill_rect};

thread_local! {
    // Fonts for the welcome screen.
    static TITLE_FONT: HFONT = unsafe { ui_font(40, true) };
    static SUBTITLE_FONT: HFONT = unsafe { ui_font(18, false) };
    static BUTTON_FONT: HFONT = unsafe { ui_font(20, false) };
}

use windows::Win32::Graphics::Gdi::HFONT;

// Create a Segoe UI font at the given pixel height.
unsafe fn ui_font(height: i32, bold: bool) -> HFONT {
    let mut lf = LOGFONTW {
        lfHeight: -height,
        lfWeight: if bold { 700 } else { 400 },
        ..Default::default()
    };
    for (slot, ch) in lf.lfFaceName.iter_mut().zip("Segoe UI".encode_utf16()) {
        *slot = ch;
    }
    CreateFontIndirectW(&lf)
}

fn welcome_bg(dark: bool) -> (u8, u8, u8) {
    if dark {
        (30, 30, 30)
    } else {
        (255, 255, 255)
    }
}

fn welcome_title_color(dark: bool) -> (u8, u8, u8) {
    if dark {
        (235, 235, 235)
    } else {
        (24, 24, 27)
    }
}

fn welcome_subtitle_color(dark: bool) -> (u8, u8, u8) {
    if dark {
        (150, 150, 150)
    } else {
        (101, 117, 133)
    }
}

pub(crate) unsafe fn welcome_visible(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().welcome_visible)
}

// Create the two owner-drawn buttons and load the display icon. Called once
// from WM_CREATE; the controls start hidden and are shown via the welcome view.
pub(crate) unsafe fn create_welcome_controls(hwnd: HWND) {
    let instance = HINSTANCE(GetModuleHandleW(None).map_or(null_mut(), |m| m.0));

    let make_button = |label: PCWSTR, id: usize| -> HWND {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            label,
            WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            0,
            0,
            WELCOME_BTN_W,
            WELCOME_BTN_H,
            hwnd,
            HMENU(id as *mut _),
            instance,
            None,
        )
        .unwrap_or(HWND(null_mut()))
    };

    let open = make_button(w!("Open"), ID_WELCOME_OPEN);
    let edit = make_button(w!("Edit"), ID_WELCOME_EDIT);

    // Load the embedded app icon (resource id 1) at display size.
    let icon = LoadImageW(
        instance,
        PCWSTR(1 as *const u16),
        IMAGE_ICON,
        WELCOME_ICON,
        WELCOME_ICON,
        LR_DEFAULTCOLOR,
    )
    .map(|handle| HICON(handle.0))
    .unwrap_or(HICON(null_mut()));

    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.welcome_open = open;
        state.welcome_edit = edit;
        state.welcome_icon = icon;
    }
}

// Position the two buttons in the right half of the window.
pub(crate) unsafe fn layout_welcome(hwnd: HWND, width: i32, height: i32) {
    let (open, edit) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (state.welcome_open, state.welcome_edit)
        }
        None => return,
    };
    if open.0.is_null() || edit.0.is_null() {
        return;
    }

    let right_center = width * 3 / 4;
    let btn_x = (right_center - WELCOME_BTN_W / 2).max(0);
    let total_h = WELCOME_BTN_H * 2 + WELCOME_BTN_GAP;
    let top = (height - total_h) / 2;

    let _ = MoveWindow(open, btn_x, top, WELCOME_BTN_W, WELCOME_BTN_H, true);
    let _ = MoveWindow(
        edit,
        btn_x,
        top + WELCOME_BTN_H + WELCOME_BTN_GAP,
        WELCOME_BTN_W,
        WELCOME_BTN_H,
        true,
    );
}

pub(crate) unsafe fn show_welcome(hwnd: HWND) {
    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.current_file = None;
        state.about_visible = false;
        state.learn_visible = false;
    }
    set_welcome_visible(hwnd, true);
    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(hwnd, APP_TITLE);
}

// Toggle the welcome controls and the document view (they are mutually
// exclusive: the welcome screen owns the client area, otherwise the RichEdit
// does).
pub(crate) unsafe fn set_welcome_visible(hwnd: HWND, visible: bool) {
    let (open, edit, rich_edit) = match state(hwnd) {
        Some(state) => {
            let mut state = state.borrow_mut();
            state.welcome_visible = visible;
            (state.welcome_open, state.welcome_edit, state.rich_edit)
        }
        None => return,
    };

    let show = if visible { SW_SHOW } else { SW_HIDE };
    if !open.0.is_null() {
        let _ = ShowWindow(open, show);
    }
    if !edit.0.is_null() {
        let _ = ShowWindow(edit, show);
    }
    if !rich_edit.0.is_null() {
        let _ = ShowWindow(rich_edit, if visible { SW_HIDE } else { SW_SHOW });
    }

    if visible {
        let _ = InvalidateRect(hwnd, None, true);
        if !open.0.is_null() {
            let _ = InvalidateRect(open, None, true);
        }
        if !edit.0.is_null() {
            let _ = InvalidateRect(edit, None, true);
        }
    }
}

pub(crate) unsafe fn erase_welcome_background(hwnd: HWND, hdc: HDC) {
    let mut rc = RECT::default();
    if GetClientRect(hwnd, &mut rc).is_err() {
        return;
    }
    let brush = CreateSolidBrush(colorref(welcome_bg(current_dark(hwnd))));
    fill_rect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

// Paint the left pane: app icon and name (background already erased).
pub(crate) unsafe fn paint_welcome(hwnd: HWND) {
    let dark = current_dark(hwnd);
    let icon = state(hwnd).map_or(HICON(null_mut()), |s| s.borrow().welcome_icon);

    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let width = rc.right - rc.left;
    let height = rc.bottom - rc.top;

    // Left pane is the left half; center the icon + title block within it.
    let left_center = width / 4;
    let block_h = WELCOME_ICON + 24 + 48; // icon + gap + title/subtitle area
    let icon_top = ((height - block_h) / 2).max(VIEW_PADDING);

    if !icon.0.is_null() {
        let _ = DrawIconEx(
            hdc,
            left_center - WELCOME_ICON / 2,
            icon_top,
            icon,
            WELCOME_ICON,
            WELCOME_ICON,
            0,
            HBRUSH(null_mut()),
            DI_NORMAL,
        );
    }

    SetBkMode(hdc, TRANSPARENT);

    // Title "Markd".
    let title_font = TITLE_FONT.with(|f| *f);
    let prev = SelectObject(hdc, HGDIOBJ(title_font.0));
    SetTextColor(hdc, colorref(welcome_title_color(dark)));
    let mut title: Vec<u16> = "Markd".encode_utf16().collect();
    let mut title_rc = RECT {
        left: 0,
        top: icon_top + WELCOME_ICON + 16,
        right: width / 2,
        bottom: icon_top + WELCOME_ICON + 16 + 52,
    };
    let _ = DrawTextW(
        hdc,
        &mut title,
        &mut title_rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(hdc, prev);

    // Subtitle.
    let subtitle_font = SUBTITLE_FONT.with(|f| *f);
    let prev = SelectObject(hdc, HGDIOBJ(subtitle_font.0));
    SetTextColor(hdc, colorref(welcome_subtitle_color(dark)));
    let mut subtitle: Vec<u16> = "Markdown Viewer".encode_utf16().collect();
    let mut subtitle_rc = RECT {
        left: 0,
        top: title_rc.bottom,
        right: width / 2,
        bottom: title_rc.bottom + 28,
    };
    let _ = DrawTextW(
        hdc,
        &mut subtitle,
        &mut subtitle_rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(hdc, prev);

    let _ = EndPaint(hwnd, &ps);
}

// Owner-draw for the welcome buttons. Returns true if it handled the item.
pub(crate) unsafe fn draw_welcome_button(lparam: windows::Win32::Foundation::LPARAM, dark: bool) -> bool {
    let dis = match (lparam.0 as *const DRAWITEMSTRUCT).as_ref() {
        Some(dis) if dis.CtlType == ODT_BUTTON => dis,
        _ => return false,
    };

    let primary = dis.CtlID as usize == ID_WELCOME_OPEN;
    let pressed = dis.itemState.0 & ODS_SELECTED.0 != 0;
    let hdc = dis.hDC;
    let rc = dis.rcItem;

    // Resolve colors: a filled accent button for "Open", a neutral/outlined
    // button for "Edit".
    let accent = if pressed { (0, 95, 184) } else { (0, 120, 212) };
    let (fill, border, text) = if primary {
        (accent, accent, (255u8, 255u8, 255u8))
    } else if dark {
        let f = if pressed { (60, 60, 60) } else { (45, 45, 45) };
        (f, (90, 90, 90), (220, 220, 220))
    } else {
        let f = if pressed { (224, 224, 224) } else { (243, 243, 243) };
        (f, (200, 200, 200), (24, 24, 27))
    };

    let brush = CreateSolidBrush(colorref(fill));
    let pen = CreatePen(PS_SOLID, 1, colorref(border));
    let prev_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let prev_pen = SelectObject(hdc, HGDIOBJ(pen.0));
    let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 10, 10);
    SelectObject(hdc, prev_brush);
    SelectObject(hdc, prev_pen);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let _ = DeleteObject(HGDIOBJ(pen.0));

    // Label from the button's window text.
    let mut label = [0u16; 64];
    let len = GetWindowTextW(dis.hwndItem, &mut label);
    let mut text_buf = label[..len as usize].to_vec();

    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, colorref(text));
    let font = BUTTON_FONT.with(|f| *f);
    let prev_font = SelectObject(hdc, HGDIOBJ(font.0));
    let mut text_rc = rc;
    let _ = DrawTextW(
        hdc,
        &mut text_buf,
        &mut text_rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(hdc, prev_font);

    true
}
