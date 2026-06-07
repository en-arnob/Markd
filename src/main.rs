#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{copy_nonoverlapping, null_mut};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, MAX_PATH, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateFontIndirectW, CreateRectRgnIndirect, CreateSolidBrush, DeleteObject,
    DrawTextW, FillRgn, GetDC, GetStockObject, GetSysColor, GetTextExtentPoint32W, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, UpdateWindow, COLOR_GRAYTEXT, COLOR_HIGHLIGHT,
    COLOR_HIGHLIGHTTEXT, COLOR_MENU, COLOR_MENUTEXT, DT_CENTER, DT_HIDEPREFIX, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, HBRUSH, HFONT, HGDIOBJ, RGN_DIFF, TRANSPARENT, WHITE_BRUSH,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Controls::RichEdit::{
    CHARRANGE, EDITSTREAM, EM_GETTEXTRANGE, EM_SETBKGNDCOLOR, EM_SETEVENTMASK, EM_STREAMIN, ENLINK,
    ENM_LINK, EN_LINK, SF_RTF, TEXTRANGEW,
};
use windows::Win32::UI::Controls::EM_SETRECT;
use windows::Win32::UI::Controls::NMHDR;
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_CHECKED, ODS_GRAYED, ODS_HOTLIGHT, ODS_SELECTED,
    ODT_MENU,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateMenu, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, DrawMenuBar, GetMenu, GetMenuBarInfo, GetMenuItemCount, GetMenuItemInfoW,
    GetMenuItemRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    LoadCursorW, MessageBoxW, MoveWindow, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor,
    SetMenu, SetMenuItemInfoW, SetWindowLongPtrW, SetWindowTextW, ShowWindow, SystemParametersInfoW,
    TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY,
    GWLP_USERDATA, HICON, IDC_ARROW, LoadIconW, MENUBARINFO, MENUITEMINFOW, MF_BYCOMMAND,
    MF_CHECKED, MF_POPUP,
    MF_STRING, MF_UNCHECKED, MFT_OWNERDRAW, MIIM_DATA, MIIM_FTYPE,
    MNC_EXECUTE, MSG, NONCLIENTMETRICSW, OBJID_MENU, SM_CXMENUCHECK, SM_CYMENU,
    SPI_GETNONCLIENTMETRICS, SW_SHOW, SW_SHOWNORMAL, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DRAWITEM, WM_LBUTTONDOWN,
    WM_MEASUREITEM, WM_MENUCHAR, WM_NCCREATE, WM_NCPAINT, WM_NOTIFY, WM_SETCURSOR, WM_SIZE,
    WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_VSCROLL,
};

const APP_CLASS: PCWSTR = w!("MarkdWindow");
const APP_TITLE: PCWSTR = w!("Markd");
const ID_FILE_OPEN: usize = 1001;
const ID_FILE_EXIT: usize = 1002;
const ID_HELP_ABOUT: usize = 2001;
const ID_SETTINGS_DARKMODE: usize = 3001;
const VIEW_PADDING: i32 = 24;

// RichEdit background colors (COLORREF, 0x00BBGGRR).
const LIGHT_BG: isize = 0x00FF_FFFF; // white
const DARK_BG: isize = 0x001E_1E1E; // #1e1e1e

// Dark menu palette (R, G, B).
const MENU_BAR_BG: (u8, u8, u8) = (43, 43, 43); // #2b2b2b
const MENU_HOT_BG: (u8, u8, u8) = (60, 60, 60); // #3c3c3c
const MENU_TEXT: (u8, u8, u8) = (220, 220, 220); // #dcdcdc
const MENU_TEXT_DISABLED: (u8, u8, u8) = (120, 120, 120);

struct AppState {
    rich_edit: HWND,
    current_file: Option<PathBuf>,
    about_visible: bool,
    dark_mode: bool,
}

struct RtfStream {
    data: Vec<u8>,
    position: usize,
}

// Backing data for an owner-drawn (dark) menu item. The pointer to this struct
// is stashed in the item's dwItemData so WM_MEASUREITEM / WM_DRAWITEM can read
// the label and how to render it. Allocated once per item and never freed (it
// lives for the lifetime of the menu / process).
struct MenuLabel {
    text: Vec<u16>,
    is_bar: bool,
}

thread_local! {
    static OPEN_FILTER: Vec<u16> = wide_filter(&[
        ("Markdown files", "*.md;*.markdown;*.mdown;*.mkd"),
        ("Text files", "*.txt"),
        ("All files", "*.*"),
    ]);

    // Cached system menu font, used to measure and paint owner-drawn menu items.
    static MENU_FONT: HFONT = unsafe { menu_font() };
}

fn main() -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        LoadLibraryW(w!("Msftedit.dll"))?;

        let instance = HINSTANCE(GetModuleHandleW(None)?.0);
        register_window_class(instance)?;
        let initial_file = std::env::args_os().nth(1).map(PathBuf::from);

        let state = Box::new(RefCell::new(AppState {
            rich_edit: HWND(null_mut()),
            current_file: None,
            about_visible: false,
            dark_mode: false,
        }));

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            APP_CLASS,
            APP_TITLE,
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            960,
            720,
            None,
            None,
            instance,
            Some(Box::into_raw(state).cast()),
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        if let Some(path) = initial_file {
            load_markdown(hwnd, &path);
        } else {
            set_rtf(hwnd, welcome_rtf(false));
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

unsafe fn register_window_class(instance: HINSTANCE) -> windows::core::Result<()> {
    // Resource id "1" matches the icon embedded by build.rs; used for the title
    // bar, taskbar, and Alt-Tab. (The same resource is the file/exe icon.)
    let icon = LoadIconW(instance, PCWSTR(1 as *const u16)).unwrap_or(HICON(null_mut()));

    let class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: icon,
        hInstance: instance,
        lpszClassName: APP_CLASS,
        hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
        lpfnWndProc: Some(window_proc),
        ..Default::default()
    };

    if RegisterClassW(&class) == 0 {
        return Err(windows::core::Error::from_win32());
    }

    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            let state = (*create).lpCreateParams as *mut RefCell<AppState>;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            LRESULT(1)
        }
        WM_CREATE => {
            create_menu(hwnd);
            if let Some(state) = state(hwnd) {
                let rich_edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    w!("RICHEDIT50W"),
                    None,
                    WS_CHILD
                        | WS_VISIBLE
                        | WS_VSCROLL
                        | WINDOW_STYLE(ES_MULTILINE as u32)
                        | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
                        | WINDOW_STYLE(ES_READONLY as u32),
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    None,
                    HINSTANCE(GetModuleHandleW(None).map_or(null_mut(), |module| module.0)),
                    None,
                );
                if let Ok(rich_edit) = rich_edit {
                    SendMessageW(
                        rich_edit,
                        EM_SETEVENTMASK,
                        WPARAM(0),
                        LPARAM(ENM_LINK as isize),
                    );
                    state.borrow_mut().rich_edit = rich_edit;
                }
            }
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(state) = state(hwnd) {
                let width = (lparam.0 & 0xffff) as i32;
                let height = ((lparam.0 >> 16) & 0xffff) as i32;
                let rich_edit = state.borrow().rich_edit;
                if !rich_edit.0.is_null() {
                    let _ = MoveWindow(rich_edit, 0, 0, width, height, true);
                    set_view_padding(rich_edit, width, height);
                }
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            handle_notification(hwnd, lparam);
            LRESULT(0)
        }
        WM_MEASUREITEM => {
            if measure_menu_item(lparam) {
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_DRAWITEM => {
            if draw_menu_item(lparam, current_dark(hwnd)) {
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_MENUCHAR => {
            // Owner-drawn items lose automatic mnemonic handling, so resolve the
            // accelerator key ourselves and tell the menu which item to run.
            if let Some(result) = handle_menu_char(wparam, lparam) {
                return result;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_NCPAINT => {
            // Let the frame paint normally, then repaint the menu-bar background
            // strip (the gaps the system fills with COLOR_MENU) so it matches
            // the owner-drawn items in the current theme.
            let result = DefWindowProcW(hwnd, message, wparam, lparam);
            paint_menu_bar_background(hwnd, current_dark(hwnd));
            result
        }
        WM_SETCURSOR => {
            if let Some(state) = state(hwnd) {
                if state.borrow().about_visible {
                    if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                        let _ = SetCursor(cursor);
                        return LRESULT(1);
                    }
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                ID_FILE_OPEN => {
                    if let Some(path) = choose_markdown_file(hwnd) {
                        load_markdown(hwnd, &path);
                    }
                }
                ID_FILE_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                ID_HELP_ABOUT => {
                    show_about(hwnd);
                }
                ID_SETTINGS_DARKMODE => {
                    toggle_dark_mode(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<AppState>;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn state(hwnd: HWND) -> Option<&'static RefCell<AppState>> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<AppState>;
    ptr.as_ref()
}

unsafe fn create_menu(hwnd: HWND) {
    let menu = CreateMenu().unwrap_or_default();
    let file_menu = CreateMenu().unwrap_or_default();
    let settings_menu = CreateMenu().unwrap_or_default();
    let help_menu = CreateMenu().unwrap_or_default();

    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_OPEN, w!("&Open..."));
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_EXIT, w!("E&xit"));
    let _ = AppendMenuW(settings_menu, MF_STRING, ID_SETTINGS_DARKMODE, w!("&Dark Mode"));
    let _ = AppendMenuW(help_menu, MF_STRING, ID_HELP_ABOUT, w!("&About"));
    let _ = AppendMenuW(menu, MF_POPUP, file_menu.0 as usize, w!("&File"));
    let _ = AppendMenuW(menu, MF_POPUP, settings_menu.0 as usize, w!("&Settings"));
    let _ = AppendMenuW(menu, MF_POPUP, help_menu.0 as usize, w!("&Help"));
    let _ = SetMenu(hwnd, menu);

    // Stash render data on every item so we can owner-draw them in dark mode.
    // Items stay MFT_STRING until dark mode is turned on.
    attach_label(menu, 0, true, "&File", true);
    attach_label(menu, 1, true, "&Settings", true);
    attach_label(menu, 2, true, "&Help", true);
    attach_label(file_menu, ID_FILE_OPEN as u32, false, "&Open...", false);
    attach_label(file_menu, ID_FILE_EXIT as u32, false, "E&xit", false);
    attach_label(settings_menu, ID_SETTINGS_DARKMODE as u32, false, "&Dark Mode", false);
    attach_label(help_menu, ID_HELP_ABOUT as u32, false, "&About", false);
}

unsafe fn show_about(hwnd: HWND) {
    let dark = current_dark(hwnd);
    set_rtf(hwnd, about_rtf(dark));
    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.current_file = None;
        state.about_visible = true;
    }
    let _ = SetWindowTextW(hwnd, w!("Markd - About"));
    // Move focus off the read-only RichEdit so it stops showing a blinking
    // caret on the About page. Links remain clickable without focus.
    let _ = SetFocus(hwnd);
}

unsafe fn current_dark(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().dark_mode)
}

unsafe fn toggle_dark_mode(hwnd: HWND) {
    let dark = {
        let Some(state) = state(hwnd) else {
            return;
        };
        let mut state = state.borrow_mut();
        state.dark_mode = !state.dark_mode;
        state.dark_mode
    };

    // Reflect the new state in the menu check mark.
    let menu = GetMenu(hwnd);
    let check = if dark { MF_CHECKED } else { MF_UNCHECKED };
    CheckMenuItem(menu, ID_SETTINGS_DARKMODE as u32, (MF_BYCOMMAND | check).0);

    apply_title_bar(hwnd, dark);
    apply_menu_theme(hwnd);
    apply_background(hwnd, dark);
    refresh_view(hwnd, dark);
}

// Dark/light title bar via DWM. Supported on Windows 10 1809+ / Windows 11.
unsafe fn apply_title_bar(hwnd: HWND, dark: bool) {
    let enabled = BOOL(dark as i32);
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &enabled as *const BOOL as *const _,
        size_of::<BOOL>() as u32,
    );
}

// Items are always owner-drawn; switching themes is just a repaint with the
// new colors. WM_DRAWITEM / WM_NCPAINT read the current theme to pick colors.
unsafe fn apply_menu_theme(hwnd: HWND) {
    let _ = DrawMenuBar(hwnd);
}

unsafe fn apply_background(hwnd: HWND, dark: bool) {
    if let Some(state) = state(hwnd) {
        let rich_edit = state.borrow().rich_edit;
        if !rich_edit.0.is_null() {
            let color = if dark { DARK_BG } else { LIGHT_BG };
            SendMessageW(rich_edit, EM_SETBKGNDCOLOR, WPARAM(0), LPARAM(color));
        }
    }
}

// Re-render whatever is currently on screen using the given theme.
unsafe fn refresh_view(hwnd: HWND, dark: bool) {
    let (current_file, about_visible) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (state.current_file.clone(), state.about_visible)
        }
        None => return,
    };

    if let Some(path) = current_file {
        load_markdown(hwnd, &path);
    } else if about_visible {
        show_about(hwnd);
    } else {
        set_rtf(hwnd, welcome_rtf(dark));
    }
}

fn colorref(rgb: (u8, u8, u8)) -> COLORREF {
    let (r, g, b) = rgb;
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
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

// Allocate the render data for a menu item, mark it owner-drawn, and stash a
// pointer to the data in the item's dwItemData (leaked for the process
// lifetime). Every item is owner-drawn in both light and dark mode so the two
// themes share identical layout — only the colors differ at paint time.
unsafe fn attach_label(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    ident: u32,
    by_position: bool,
    label: &str,
    is_bar: bool,
) {
    let data = Box::new(MenuLabel {
        text: to_wide(label),
        is_bar,
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
unsafe fn measure_menu_item(lparam: LPARAM) -> bool {
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
    let size = text_extent(&strip_mnemonic(&label.text));
    if label.is_bar {
        mis.itemWidth = (size.cx + 8).max(0) as u32;
        mis.itemHeight = GetSystemMetrics(SM_CYMENU).max(size.cy) as u32;
    } else {
        mis.itemWidth = (menu_gutter() + size.cx + 16).max(0) as u32;
        mis.itemHeight = (size.cy + 8).max(GetSystemMetrics(SM_CYMENU)) as u32;
    }
    true
}

// WM_DRAWITEM: paint an owner-drawn menu item. Returns true if handled.
unsafe fn draw_menu_item(lparam: LPARAM, dark: bool) -> bool {
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
        let _ = DrawTextW(
            hdc,
            &mut text,
            &mut text_rc,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX,
        );
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

unsafe fn fill_rect(hdc: windows::Win32::Graphics::Gdi::HDC, rc: &RECT, brush: HBRUSH) {
    windows::Win32::Graphics::Gdi::FillRect(hdc, rc, brush);
}

// Paint the menu-bar strip behind/around the owner-drawn items in the current
// theme, so the gaps match the items.
unsafe fn paint_menu_bar_background(hwnd: HWND, dark: bool) {
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

    let hdc = windows::Win32::Graphics::Gdi::GetWindowDC(hwnd);
    if !hdc.0.is_null() {
        let bar_bg = menu_colors(dark).bg;
        let brush = CreateSolidBrush(bar_bg);
        let _ = FillRgn(hdc, region, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        ReleaseDC(hwnd, hdc);
    }
    let _ = DeleteObject(HGDIOBJ(region.0));
}

fn offset_rect(rc: RECT, origin: POINT) -> RECT {
    RECT {
        left: rc.left - origin.x,
        top: rc.top - origin.y,
        right: rc.right - origin.x,
        bottom: rc.bottom - origin.y,
    }
}

// Match a typed mnemonic (Alt+letter) against owner-drawn items and tell the
// menu to execute the matching one. Returns None when nothing matches.
unsafe fn handle_menu_char(wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let menu = windows::Win32::UI::WindowsAndMessaging::HMENU(lparam.0 as *mut _);
    let typed = char::from_u32((wparam.0 & 0xffff) as u32)?
        .to_ascii_lowercase();

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

// The character following '&' in a label, lowercased.
fn mnemonic_char(text: &[u16]) -> Option<char> {
    let amp = u16::from(b'&');
    let pos = text.iter().position(|&c| c == amp)?;
    let next = *text.get(pos + 1)?;
    char::from_u32(next as u32).map(|c| c.to_ascii_lowercase())
}

// Drop the first '&' mnemonic marker (it's drawn as an underline, not a glyph)
// so the label measures at its visible width. A trailing null is preserved.
fn strip_mnemonic(text: &[u16]) -> Vec<u16> {
    let amp = u16::from(b'&');
    if let Some(pos) = text.iter().position(|&c| c == amp) {
        let mut out = Vec::with_capacity(text.len() - 1);
        out.extend_from_slice(&text[..pos]);
        out.extend_from_slice(&text[pos + 1..]);
        out
    } else {
        text.to_vec()
    }
}

unsafe fn handle_notification(hwnd: HWND, lparam: LPARAM) {
    let nmhdr = (lparam.0 as *const NMHDR).as_ref();
    if !matches!(nmhdr, Some(nmhdr) if nmhdr.code == EN_LINK) {
        return;
    }

    let link = lparam.0 as *const ENLINK;
    let msg = std::ptr::addr_of!((*link).msg).read_unaligned();
    if msg != WM_LBUTTONDOWN {
        return;
    }

    let chrg = std::ptr::addr_of!((*link).chrg).read_unaligned();
    if let Some(label) = link_text(hwnd, chrg) {
        // The text range reported by EN_LINK for a friendly-name hyperlink can
        // contain the hidden URL run as well as (or instead of) the visible
        // label, so match on stable substrings rather than the exact text.
        let label = label.to_lowercase();
        if label.contains("khalid utsob") || label.contains("khalidutsob") {
            open_url(hwnd, "https://khalidutsob.com");
        } else if label.contains("en-arnob") {
            open_url(hwnd, "https://github.com/en-arnob/markd");
        }
    }
}

unsafe fn link_text(hwnd: HWND, chrg: CHARRANGE) -> Option<String> {
    let state = state(hwnd)?;
    let rich_edit = state.borrow().rich_edit;
    if rich_edit.0.is_null() || chrg.cpMax <= chrg.cpMin {
        return None;
    }

    let mut text = vec![0u16; (chrg.cpMax - chrg.cpMin + 1) as usize];
    let mut text_range = TEXTRANGEW {
        chrg,
        lpstrText: PWSTR(text.as_mut_ptr()),
    };

    SendMessageW(
        rich_edit,
        EM_GETTEXTRANGE,
        WPARAM(0),
        LPARAM((&mut text_range as *mut TEXTRANGEW) as isize),
    );

    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..len]))
}

unsafe fn open_url(hwnd: HWND, url: &str) {
    let wide_url = to_wide(url);
    let _ = ShellExecuteW(
        hwnd,
        w!("open"),
        PCWSTR(wide_url.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
}

unsafe fn set_view_padding(rich_edit: HWND, width: i32, height: i32) {
    let mut rect = RECT {
        left: VIEW_PADDING,
        top: VIEW_PADDING,
        right: (width - VIEW_PADDING).max(VIEW_PADDING),
        bottom: (height - VIEW_PADDING).max(VIEW_PADDING),
    };

    SendMessageW(
        rich_edit,
        EM_SETRECT,
        WPARAM(0),
        LPARAM((&mut rect as *mut RECT) as isize),
    );
}

unsafe fn choose_markdown_file(hwnd: HWND) -> Option<PathBuf> {
    let mut file_name = [0u16; MAX_PATH as usize];
    let filter = OPEN_FILTER.with(|filter| filter.as_ptr());
    let mut open_file_name = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter),
        lpstrFile: PWSTR(file_name.as_mut_ptr()),
        nMaxFile: file_name.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY,
        ..Default::default()
    };

    if GetOpenFileNameW(&mut open_file_name).as_bool() {
        let len = file_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(file_name.len());
        Some(PathBuf::from(String::from_utf16_lossy(&file_name[..len])))
    } else {
        None
    }
}

fn load_markdown(hwnd: HWND, path: &Path) {
    match fs::read_to_string(path) {
        Ok(markdown) => {
            let dark = unsafe { current_dark(hwnd) };
            let rtf = markdown_to_rtf(&markdown, dark);
            unsafe {
                set_rtf(hwnd, rtf);
                let mut rich_edit = HWND(null_mut());
                if let Some(state) = state(hwnd) {
                    let mut state = state.borrow_mut();
                    state.current_file = Some(path.to_path_buf());
                    state.about_visible = false;
                    rich_edit = state.rich_edit;
                }
                if !rich_edit.0.is_null() {
                    let _ = SetFocus(rich_edit);
                }
                let title = format!("Markd - {}", path.display());
                let wide_title = to_wide(&title);
                let _ = SetWindowTextW(hwnd, PCWSTR(wide_title.as_ptr()));
            }
        }
        Err(error) => unsafe {
            let message = format!("Could not open file:\n{}\n\n{}", path.display(), error);
            let wide_message = to_wide(&message);
            let _ = MessageBoxW(
                hwnd,
                PCWSTR(wide_message.as_ptr()),
                w!("Markd"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        },
    }
}

unsafe fn set_rtf(hwnd: HWND, rtf: String) {
    if let Some(state) = state(hwnd) {
        let rich_edit = state.borrow().rich_edit;
        if rich_edit.0.is_null() {
            return;
        }

        let mut rtf_stream = RtfStream {
            data: rtf.into_bytes(),
            position: 0,
        };
        let mut edit_stream = EDITSTREAM {
            dwCookie: (&mut rtf_stream as *mut RtfStream) as usize,
            dwError: 0,
            pfnCallback: Some(rtf_stream_callback),
        };

        SendMessageW(
            rich_edit,
            EM_STREAMIN,
            WPARAM(SF_RTF as usize),
            LPARAM((&mut edit_stream as *mut EDITSTREAM) as isize),
        );
    }
}

unsafe extern "system" fn rtf_stream_callback(
    cookie: usize,
    buffer: *mut u8,
    buffer_len: i32,
    bytes_written: *mut i32,
) -> u32 {
    let stream = &mut *(cookie as *mut RtfStream);
    let remaining = stream.data.len().saturating_sub(stream.position);
    let count = remaining.min(buffer_len as usize);

    if count > 0 {
        copy_nonoverlapping(stream.data.as_ptr().add(stream.position), buffer, count);
        stream.position += count;
    }

    *bytes_written = count as i32;
    0
}

// Color table used by every document. The three entries map to:
//   cf1 = body text, cf2 = links/accent, cf3 = inline-code highlight background.
fn color_table(dark: bool) -> &'static str {
    if dark {
        r"{\colortbl;\red220\green220\blue220;\red88\green166\blue255;\red60\green60\blue60;}"
    } else {
        r"{\colortbl;\red24\green24\blue27;\red101\green117\blue133;\red246\green248\blue250;}"
    }
}

fn rtf_header(dark: bool) -> String {
    format!(
        r"{{\rtf1\ansi\deff0{{\fonttbl{{\f0 Segoe UI;}}{{\f1 Consolas;}}}}{}\paperw12240\paperh15840\margl720\margr720\viewkind4\uc1",
        color_table(dark)
    )
}

fn welcome_rtf(dark: bool) -> String {
    format!(
        r"{}\pard\cf1\sa220\f0\fs36\b Markd\b0\par\fs22 Open a Markdown file with File > Open.\par}}",
        rtf_header(dark)
    )
}

fn about_rtf(dark: bool) -> String {
    format!(
        r#"{}\pard\cf1\f0\fs40\b Markd\b0\par\pard\sa240\fs22 Lightweight native Markdown viewer for Windows, built with Rust for speed, simplicity, and efficiency.\par\pard\sa140\b Author:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://khalidutsob.com"}}}}{{\fldrslt{{\cf2\ul Khalid Utsob}}}}}}\ul0\cf1\par\pard\sa140\b GitHub:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://github.com/en-arnob/markd"}}}}{{\fldrslt{{\cf2\ul en-arnob/markd}}}}}}\ul0\cf1\par\pard\sa140\b Version:\b0  1.0.3\par}}"#,
        rtf_header(dark)
    )
}

fn markdown_to_rtf(markdown: &str, dark: bool) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut out = rtf_header(dark);
    out.push_str(r"\pard\cf1\f0\fs22 ");
    let mut list_depth = 0usize;
    let mut in_code_block = false;
    let mut in_table_cell = false;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => out.push_str(r"\pard\sa180\cf1\f0\fs22 "),
                Tag::Heading { level, .. } => {
                    let size = heading_size(level);
                    out.push_str(&format!(r"\pard\sa220\b\fs{} ", size));
                }
                Tag::BlockQuote(_) => out.push_str(r"\pard\li360\sa180\i\cf2 "),
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                            format!("{}\\line ", escape_rtf(&lang))
                        }
                        _ => String::new(),
                    };
                    out.push_str(r"\pard\li240\ri240\sa200\cf1\f1\fs20 ");
                    if !language.is_empty() {
                        out.push_str(r"\b ");
                        out.push_str(&language);
                        out.push_str(r"\b0 ");
                    }
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    let indent = list_depth.saturating_mul(360);
                    out.push_str(&format!(r"\pard\li{}\sa80 \bullet\tab ", indent));
                }
                Tag::Emphasis => out.push_str(r"\i "),
                Tag::Strong => out.push_str(r"\b "),
                Tag::Strikethrough => out.push_str(r"\strike "),
                Tag::Link { dest_url, .. } => {
                    out.push_str(r"\cf2\ul ");
                    if !dest_url.is_empty() {
                        out.push_str(&escape_rtf(&dest_url));
                        out.push_str(" - ");
                    }
                }
                Tag::Table(_) => out.push_str(r"\pard\sa160 "),
                Tag::TableHead => out.push_str(r"\b "),
                Tag::TableRow => {}
                Tag::TableCell => {
                    in_table_cell = true;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => out.push_str(r"\par "),
                TagEnd::Heading(_) => out.push_str(r"\b0\fs22\par "),
                TagEnd::BlockQuote(_) => out.push_str(r"\i0\cf1\par "),
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    out.push_str(r"\par ");
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    out.push_str(r"\par ");
                }
                TagEnd::Item => out.push_str(r"\par "),
                TagEnd::Emphasis => out.push_str(r"\i0 "),
                TagEnd::Strong => out.push_str(r"\b0 "),
                TagEnd::Strikethrough => out.push_str(r"\strike0 "),
                TagEnd::Link => out.push_str(r"\ul0\cf1 "),
                TagEnd::Table => out.push_str(r"\par "),
                TagEnd::TableHead => out.push_str(r"\b0\par "),
                TagEnd::TableRow => out.push_str(r"\par "),
                TagEnd::TableCell => {
                    in_table_cell = false;
                    out.push_str(r"\tab ");
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&escape_rtf(&text).replace('\n', r"\line "));
                } else {
                    out.push_str(&escape_rtf(&text));
                }
                if in_table_cell {
                    out.push(' ');
                }
            }
            Event::Code(code) => {
                out.push_str(r"\f1\highlight3 ");
                out.push_str(&escape_rtf(&code));
                out.push_str(r"\highlight0\f0 ");
            }
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push_str(r"\line "),
            Event::Rule => {
                out.push_str(r"\pard\sa180 ________________________________________\par ")
            }
            Event::TaskListMarker(checked) => {
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::Html(html) | Event::InlineHtml(html) => out.push_str(&escape_rtf(&html)),
            Event::FootnoteReference(note) => {
                out.push('[');
                out.push_str(&escape_rtf(&note));
                out.push(']');
            }
            _ => {}
        }
    }

    out.push('}');
    out
}

fn heading_size(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 40,
        HeadingLevel::H2 => 32,
        HeadingLevel::H3 => 28,
        HeadingLevel::H4 => 24,
        HeadingLevel::H5 => 22,
        HeadingLevel::H6 => 20,
    }
}

fn escape_rtf(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str(r"\\"),
            '{' => escaped.push_str(r"\{"),
            '}' => escaped.push_str(r"\}"),
            '\n' => escaped.push_str(r"\line "),
            '\r' => {}
            ch if ch.is_ascii() => escaped.push(ch),
            ch => escaped.push_str(&format!(r"\u{}?", ch as i32)),
        }
    }
    escaped
}

fn to_wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

fn wide_filter(filters: &[(&str, &str)]) -> Vec<u16> {
    let mut out = Vec::new();
    for (label, pattern) in filters {
        out.extend(OsStr::new(label).encode_wide());
        out.push(0);
        out.extend(OsStr::new(pattern).encode_wide());
        out.push(0);
    }
    out.push(0);
    out
}
