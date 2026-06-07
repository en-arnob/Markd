//! The application menu bar: building it, owner-drawing every item so the dark
//! theme matches, and resolving Alt-mnemonics for owner-drawn items.

use std::mem::size_of;
use std::ptr::null_mut;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateFontIndirectW, CreateRectRgnIndirect, CreateSolidBrush, DeleteObject,
    DrawTextW, FillRgn, GetDC, GetSysColor, GetTextExtentPoint32W, GetWindowDC, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
    COLOR_MENU, COLOR_MENUTEXT, DT_CENTER, DT_HIDEPREFIX, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, HFONT, HGDIOBJ, RGN_DIFF, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_CHECKED, ODS_GRAYED, ODS_HOTLIGHT, ODS_SELECTED, ODT_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateMenu, GetMenu, GetMenuBarInfo, GetMenuItemCount, GetMenuItemInfoW,
    GetMenuItemRect, GetSystemMetrics, GetWindowRect, SetMenu, SetMenuItemInfoW, HMENU, MENUBARINFO,
    MENUITEMINFOW, MFT_OWNERDRAW, MF_POPUP, MF_STRING, MIIM_DATA, MIIM_FTYPE, MNC_EXECUTE,
    NONCLIENTMETRICSW, OBJID_MENU, SM_CXMENUCHECK, SM_CYMENU, SPI_GETNONCLIENTMETRICS,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

use crate::app::{
    ID_FILE_EXIT, ID_FILE_OPEN, ID_FILE_SAVE, ID_HELP_ABOUT, ID_LEARN_MARKDOWN,
    ID_SETTINGS_DARKMODE, ID_SETTINGS_EDITMODE, MENU_BAR_BG, MENU_HOT_BG, MENU_TEXT,
    MENU_TEXT_DISABLED, TAB,
};
use crate::util::{colorref, fill_rect, mnemonic_char, offset_rect, strip_mnemonic, to_wide};

thread_local! {
    // Cached system menu font, used to measure and paint owner-drawn menu items.
    static MENU_FONT: HFONT = unsafe { menu_font() };
}

// Backing data for an owner-drawn menu item. The pointer to this struct is
// stashed in the item's dwItemData so WM_MEASUREITEM / WM_DRAWITEM can read the
// label and how to render it. Allocated once per item and never freed (it lives
// for the lifetime of the menu / process).
struct MenuLabel {
    text: Vec<u16>,
    is_bar: bool,
    // True for a popup item that opens a submenu; we owner-draw the ">" arrow
    // ourselves since the system doesn't draw it for owner-drawn items.
    has_submenu: bool,
}

pub(crate) unsafe fn create_menu(hwnd: HWND) {
    let menu = CreateMenu().unwrap_or_default();
    let file_menu = CreateMenu().unwrap_or_default();
    let settings_menu = CreateMenu().unwrap_or_default();
    let help_menu = CreateMenu().unwrap_or_default();
    let learn_menu = CreateMenu().unwrap_or_default();

    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_OPEN, w!("&Open..."));
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_SAVE, w!("&Save"));
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_EXIT, w!("E&xit"));
    let _ = AppendMenuW(settings_menu, MF_STRING, ID_SETTINGS_DARKMODE, w!("&Dark Mode"));
    let _ = AppendMenuW(settings_menu, MF_STRING, ID_SETTINGS_EDITMODE, w!("&Edit Mode"));
    let _ = AppendMenuW(learn_menu, MF_STRING, ID_LEARN_MARKDOWN, w!("&Markdown Basics"));
    let _ = AppendMenuW(help_menu, MF_POPUP, learn_menu.0 as usize, w!("&Learn"));
    let _ = AppendMenuW(help_menu, MF_STRING, ID_HELP_ABOUT, w!("&About"));
    let _ = AppendMenuW(menu, MF_POPUP, file_menu.0 as usize, w!("&File"));
    let _ = AppendMenuW(menu, MF_POPUP, settings_menu.0 as usize, w!("&Settings"));
    let _ = AppendMenuW(menu, MF_POPUP, help_menu.0 as usize, w!("&Help"));
    let _ = SetMenu(hwnd, menu);

    // Stash render data on every item so we can owner-draw them in both themes.
    attach_label(menu, 0, true, "&File", true, false);
    attach_label(menu, 1, true, "&Settings", true, false);
    attach_label(menu, 2, true, "&Help", true, false);
    attach_label(file_menu, ID_FILE_OPEN as u32, false, "&Open...", false, false);
    attach_label(file_menu, ID_FILE_SAVE as u32, false, "&Save", false, false);
    attach_label(file_menu, ID_FILE_EXIT as u32, false, "E&xit", false, false);
    attach_label(settings_menu, ID_SETTINGS_DARKMODE as u32, false, "&Dark Mode", false, false);
    attach_label(settings_menu, ID_SETTINGS_EDITMODE as u32, false, "&Edit Mode", false, false);
    // Learn is a popup item (by position) that opens a one-item submenu.
    attach_label(help_menu, 0, true, "&Learn", false, true);
    attach_label(learn_menu, ID_LEARN_MARKDOWN as u32, false, "&Markdown Basics", false, false);
    attach_label(help_menu, ID_HELP_ABOUT as u32, false, "&About", false, false);
}

// Allocate the render data for a menu item, mark it owner-drawn, and stash a
// pointer to the data in the item's dwItemData (leaked for the process
// lifetime). Every item is owner-drawn in both light and dark mode so the two
// themes share identical layout — only the colors differ at paint time.
unsafe fn attach_label(
    menu: HMENU,
    ident: u32,
    by_position: bool,
    label: &str,
    is_bar: bool,
    has_submenu: bool,
) {
    let data = Box::new(MenuLabel {
        text: to_wide(label),
        is_bar,
        has_submenu,
    });
    let info = MENUITEMINFOW {
        cbSize: size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_FTYPE | MIIM_DATA,
        fType: MFT_OWNERDRAW,
        dwItemData: Box::into_raw(data) as usize,
        ..Default::default()
    };
    let _ = SetMenuItemInfoW(menu, ident, by_position, &info);
}

// Resolve the system menu font so owner-drawn items match everything else.
unsafe fn menu_font() -> HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    if SystemParametersInfoW(
        SPI_GETNONCLIENTMETRICS,
        size_of::<NONCLIENTMETRICSW>() as u32,
        Some(&mut metrics as *mut _ as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .is_ok()
    {
        let font = CreateFontIndirectW(&metrics.lfMenuFont);
        if !font.0.is_null() {
            return font;
        }
    }
    HFONT(null_mut())
}

// Resolved colors for painting menu items in the current theme. Light mode
// pulls the system menu colors so it matches the rest of the OS.
struct MenuColors {
    bg: COLORREF,
    text: COLORREF,
    hot_bg: COLORREF,
    hot_text: COLORREF,
    disabled: COLORREF,
}

unsafe fn menu_colors(dark: bool) -> MenuColors {
    if dark {
        MenuColors {
            bg: colorref(MENU_BAR_BG),
            text: colorref(MENU_TEXT),
            hot_bg: colorref(MENU_HOT_BG),
            hot_text: colorref(MENU_TEXT),
            disabled: colorref(MENU_TEXT_DISABLED),
        }
    } else {
        MenuColors {
            bg: COLORREF(GetSysColor(COLOR_MENU)),
            text: COLORREF(GetSysColor(COLOR_MENUTEXT)),
            hot_bg: COLORREF(GetSysColor(COLOR_HIGHLIGHT)),
            hot_text: COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT)),
            disabled: COLORREF(GetSysColor(COLOR_GRAYTEXT)),
        }
    }
}

// Left gutter width for popup items (check column + spacing). Shared by
// measuring and drawing so the text always lines up with the reserved space.
unsafe fn menu_gutter() -> i32 {
    GetSystemMetrics(SM_CXMENUCHECK).max(16) + 6
}

// WM_MEASUREITEM: size an owner-drawn menu item. Returns true if handled.
pub(crate) unsafe fn measure_menu_item(lparam: LPARAM) -> bool {
    let mis = match (lparam.0 as *mut MEASUREITEMSTRUCT).as_mut() {
        Some(mis) if mis.CtlType == ODT_MENU => mis,
        _ => return false,
    };
    let label = match (mis.itemData as *const MenuLabel).as_ref() {
        Some(label) => label,
        None => return false,
    };

    // Measure without the '&' mnemonic marker, which is consumed (not drawn)
    // when the item is painted — otherwise items measure wider than they show.
    let content = strip_mnemonic(&label.text);
    let content = crate::util::trim_null(&content);
    if label.is_bar {
        let size = text_extent(content);
        mis.itemWidth = (size.cx + 8).max(0) as u32;
        mis.itemHeight = GetSystemMetrics(SM_CYMENU).max(size.cy) as u32;
    } else if let Some(tab) = content.iter().position(|&c| c == TAB) {
        // "Label\tAccelerator": reserve room for both plus a gap between them.
        let left = text_extent(&content[..tab]);
        let right = text_extent(&content[tab + 1..]);
        mis.itemWidth = (menu_gutter() + left.cx + 32 + right.cx + 16).max(0) as u32;
        mis.itemHeight = (left.cy.max(right.cy) + 8).max(GetSystemMetrics(SM_CYMENU)) as u32;
    } else {
        let size = text_extent(content);
        let arrow = if label.has_submenu { 16 } else { 0 };
        mis.itemWidth = (menu_gutter() + size.cx + 16 + arrow).max(0) as u32;
        mis.itemHeight = (size.cy + 8).max(GetSystemMetrics(SM_CYMENU)) as u32;
    }
    true
}

// WM_DRAWITEM: paint an owner-drawn menu item. Returns true if handled.
pub(crate) unsafe fn draw_menu_item(lparam: LPARAM, dark: bool) -> bool {
    let dis = match (lparam.0 as *const DRAWITEMSTRUCT).as_ref() {
        Some(dis) if dis.CtlType == ODT_MENU => dis,
        _ => return false,
    };
    let label = match (dis.itemData as *const MenuLabel).as_ref() {
        Some(label) => label,
        None => return false,
    };

    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let item_state = dis.itemState.0;
    let selected = item_state & (ODS_SELECTED.0 | ODS_HOTLIGHT.0) != 0;
    let disabled = item_state & ODS_GRAYED.0 != 0;
    let checked = item_state & ODS_CHECKED.0 != 0;

    let colors = menu_colors(dark);
    let brush = CreateSolidBrush(if selected { colors.hot_bg } else { colors.bg });
    fill_rect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));

    SetBkMode(hdc, TRANSPARENT);
    let text_color = if disabled {
        colors.disabled
    } else if selected {
        colors.hot_text
    } else {
        colors.text
    };
    SetTextColor(hdc, text_color);

    let font = MENU_FONT.with(|font| *font);
    let previous = if !font.0.is_null() {
        Some(SelectObject(hdc, HGDIOBJ(font.0)))
    } else {
        None
    };

    let mut text: Vec<u16> = label.text.clone();
    if text.last() == Some(&0) {
        text.pop();
    }

    if label.is_bar {
        let mut draw_rc = rc;
        let _ = DrawTextW(
            hdc,
            &mut text,
            &mut draw_rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        let gutter = menu_gutter();
        if checked {
            let mut mark: Vec<u16> = "\u{2713}".encode_utf16().collect();
            let mut check_rc = RECT {
                left: rc.left,
                top: rc.top,
                right: rc.left + gutter,
                bottom: rc.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut mark,
                &mut check_rc,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        let mut text_rc = RECT {
            left: rc.left + gutter,
            top: rc.top,
            right: rc.right - 8,
            bottom: rc.bottom,
        };
        if let Some(tab) = text.iter().position(|&c| c == TAB) {
            // Label left-aligned, accelerator hint right-aligned in the same rect.
            let mut left = text[..tab].to_vec();
            let mut right = text[tab + 1..].to_vec();
            let _ = DrawTextW(
                hdc,
                &mut left,
                &mut text_rc,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX,
            );
            let _ = DrawTextW(
                hdc,
                &mut right,
                &mut text_rc,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX,
            );
        } else {
            let _ = DrawTextW(
                hdc,
                &mut text,
                &mut text_rc,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX,
            );
        }
        if label.has_submenu {
            // Owner-drawn items don't get the system submenu arrow; draw one.
            let mut arrow: Vec<u16> = "\u{203A}".encode_utf16().collect();
            let _ = DrawTextW(
                hdc,
                &mut arrow,
                &mut text_rc,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }

    if let Some(previous) = previous {
        SelectObject(hdc, previous);
    }
    true
}

// Measure a (possibly null-terminated) wide string in the menu font.
unsafe fn text_extent(text: &[u16]) -> SIZE {
    let mut size = SIZE::default();
    let trimmed = match text.split_last() {
        Some((&0, rest)) => rest,
        _ => text,
    };
    let hdc = GetDC(None);
    if hdc.0.is_null() {
        return size;
    }
    let font = MENU_FONT.with(|font| *font);
    let previous = if !font.0.is_null() {
        Some(SelectObject(hdc, HGDIOBJ(font.0)))
    } else {
        None
    };
    let _ = GetTextExtentPoint32W(hdc, trimmed, &mut size);
    if let Some(previous) = previous {
        SelectObject(hdc, previous);
    }
    ReleaseDC(None, hdc);
    size
}

// Paint the menu-bar strip behind/around the owner-drawn items in the current
// theme, so the gaps match the items.
pub(crate) unsafe fn paint_menu_bar_background(hwnd: HWND, dark: bool) {
    let mut info = MENUBARINFO {
        cbSize: size_of::<MENUBARINFO>() as u32,
        ..Default::default()
    };
    if GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut info).is_err() {
        return;
    }

    let mut window = RECT::default();
    if GetWindowRect(hwnd, &mut window).is_err() {
        return;
    }
    let origin = POINT {
        x: window.left,
        y: window.top,
    };

    // Whole bar, in window-relative coordinates (the DC from GetWindowDC).
    let bar = offset_rect(info.rcBar, origin);
    let region = CreateRectRgnIndirect(&bar);

    // Subtract each item rect so we only fill the gaps, leaving items intact.
    let menu = GetMenu(hwnd);
    let count = GetMenuItemCount(menu);
    for i in 0..count {
        let mut item = RECT::default();
        if GetMenuItemRect(hwnd, menu, i as u32, &mut item).is_ok() {
            let item = offset_rect(item, origin);
            let item_rgn = CreateRectRgnIndirect(&item);
            CombineRgn(region, region, item_rgn, RGN_DIFF);
            let _ = DeleteObject(HGDIOBJ(item_rgn.0));
        }
    }

    let hdc = GetWindowDC(hwnd);
    if !hdc.0.is_null() {
        let bar_bg = menu_colors(dark).bg;
        let brush = CreateSolidBrush(bar_bg);
        let _ = FillRgn(hdc, region, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        ReleaseDC(hwnd, hdc);
    }
    let _ = DeleteObject(HGDIOBJ(region.0));
}

// Match a typed mnemonic (Alt+letter) against owner-drawn items and tell the
// menu to execute the matching one. Returns None when nothing matches.
pub(crate) unsafe fn handle_menu_char(wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let menu = HMENU(lparam.0 as *mut _);
    let typed = char::from_u32((wparam.0 & 0xffff) as u32)?.to_ascii_lowercase();

    let count = GetMenuItemCount(menu);
    for i in 0..count {
        let mut info = MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_DATA,
            ..Default::default()
        };
        if GetMenuItemInfoW(menu, i as u32, true, &mut info).is_err() {
            continue;
        }
        if let Some(label) = (info.dwItemData as *const MenuLabel).as_ref() {
            if let Some(mnemonic) = mnemonic_char(&label.text) {
                if mnemonic == typed {
                    // HIWORD = MNC_EXECUTE, LOWORD = item index.
                    return Some(LRESULT(((MNC_EXECUTE as isize) << 16) | i as isize));
                }
            }
        }
    }
    None
}
