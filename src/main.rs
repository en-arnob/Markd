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
    BeginPaint, CombineRgn, CreateFontIndirectW, CreatePen, CreateRectRgnIndirect, CreateSolidBrush,
    DeleteObject, DrawTextW, EndPaint, FillRgn, GetDC, GetStockObject, GetSysColor,
    GetTextExtentPoint32W, InvalidateRect, ReleaseDC, RoundRect, SelectObject, SetBkMode,
    SetTextColor, UpdateWindow, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_MENU,
    COLOR_MENUTEXT, DT_CENTER, DT_HIDEPREFIX, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HBRUSH,
    HDC, HFONT,
    HGDIOBJ, LOGFONTW, PAINTSTRUCT, PS_SOLID, RGN_DIFF, TRANSPARENT, WHITE_BRUSH,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Controls::RichEdit::{
    CHARFORMAT2W, CHARRANGE, EDITSTREAM, EM_GETTEXTRANGE, EM_SETBKGNDCOLOR, EM_SETCHARFORMAT,
    EM_SETEVENTMASK, EM_STREAMIN, ENLINK, ENM_CHANGE, ENM_LINK, EN_LINK, CFM_COLOR, CFM_FACE, CFM_MASK,
    CFM_SIZE, SCF_ALL, SF_RTF, TEXTRANGEW,
};
use windows::Win32::UI::Controls::EM_SETRECT;
use windows::Win32::UI::Controls::EM_SETREADONLY;
use windows::Win32::UI::Controls::NMHDR;
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_CHECKED, ODS_GRAYED, ODS_HOTLIGHT, ODS_SELECTED,
    ODT_BUTTON, ODT_MENU,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateAcceleratorTableW, CreateMenu, CreateWindowExW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, DrawIconEx, DrawMenuBar, GetClientRect, GetMenu, GetMenuBarInfo,
    GetMenuItemCount, GetMenuItemInfoW, GetMenuItemRect, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, LoadCursorW, LoadImageW,
    MessageBoxW,
    MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor, SetMenu,
    SetMenuItemInfoW,
    SetWindowLongPtrW, SetWindowTextW, ShowWindow, SystemParametersInfoW, TranslateAcceleratorW,
    TranslateMessage, ACCEL, ACCEL_VIRT_FLAGS, BS_OWNERDRAW, CREATESTRUCTW, CW_USEDEFAULT, DI_NORMAL,
    EN_CHANGE, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, FCONTROL, FVIRTKEY, GWLP_USERDATA, HACCEL,
    HICON, HMENU, IDC_ARROW, IDNO, IDYES, IMAGE_ICON, LR_DEFAULTCOLOR, LoadIconW,
    MB_ICONWARNING, MB_YESNOCANCEL, MENUBARINFO,
    MENUITEMINFOW, MF_BYCOMMAND, MF_CHECKED, MF_POPUP, MF_STRING, MF_UNCHECKED, MFT_OWNERDRAW,
    MIIM_DATA, MIIM_FTYPE, MNC_EXECUTE, MSG, NONCLIENTMETRICSW, OBJID_MENU, SM_CXMENUCHECK,
    SM_CYMENU, SPI_GETNONCLIENTMETRICS, SW_HIDE, SW_SHOW, SW_SHOWNORMAL,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_MEASUREITEM, WM_MENUCHAR, WM_NCCREATE,
    WM_NCPAINT, WM_NOTIFY, WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

const APP_CLASS: PCWSTR = w!("MarkdWindow");
const APP_TITLE: PCWSTR = w!("Markd");
const ID_FILE_OPEN: usize = 1001;
const ID_FILE_EXIT: usize = 1002;
const ID_FILE_SAVE: usize = 1003;
const ID_HELP_ABOUT: usize = 2001;
const ID_SETTINGS_DARKMODE: usize = 3001;
const ID_SETTINGS_EDITMODE: usize = 3002;
const ID_WELCOME_OPEN: usize = 4001;
const ID_WELCOME_EDIT: usize = 4002;
const VIEW_PADDING: i32 = 24;

// Welcome screen layout (logical pixels).
const WELCOME_ICON: i32 = 96;
const WELCOME_BTN_W: i32 = 200;
const WELCOME_BTN_H: i32 = 48;
const WELCOME_BTN_GAP: i32 = 16;

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
    // Edit mode shows the raw Markdown source as editable text; otherwise the
    // rendered document is shown read-only. `source` holds the current Markdown
    // in memory so edits survive theme/mode toggles.
    edit_mode: bool,
    source: String,
    // True when the in-memory source has unsaved edits. Reflected in the title
    // bar (a trailing " *") and used to prompt before closing.
    dirty: bool,
    // Set while we update the editor programmatically (loading/rendering) so the
    // resulting EN_CHANGE notifications don't get mistaken for user edits.
    suppress_dirty: bool,
    // Welcome (start) screen: shown when no document is open.
    welcome_visible: bool,
    welcome_open: HWND,
    welcome_edit: HWND,
    welcome_icon: HICON,
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

    // Fonts for the welcome screen.
    static TITLE_FONT: HFONT = unsafe { ui_font(40, true) };
    static SUBTITLE_FONT: HFONT = unsafe { ui_font(18, false) };
    static BUTTON_FONT: HFONT = unsafe { ui_font(20, false) };
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
            edit_mode: false,
            source: String::new(),
            dirty: false,
            suppress_dirty: false,
            welcome_visible: false,
            welcome_open: HWND(null_mut()),
            welcome_edit: HWND(null_mut()),
            welcome_icon: HICON(null_mut()),
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
            show_welcome(hwnd);
        }

        // Ctrl+S -> Save.
        let accel = [ACCEL {
            fVirt: ACCEL_VIRT_FLAGS(FCONTROL.0 | FVIRTKEY.0),
            key: b'S' as u16,
            cmd: ID_FILE_SAVE as u16,
        }];
        let accel_table = CreateAcceleratorTableW(&accel).unwrap_or(HACCEL(null_mut()));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            if accel_table.0.is_null() || TranslateAcceleratorW(hwnd, accel_table, &msg) == 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
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
                        LPARAM((ENM_LINK | ENM_CHANGE) as isize),
                    );
                    state.borrow_mut().rich_edit = rich_edit;
                }
                create_welcome_controls(hwnd);
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
            layout_welcome(hwnd, (lparam.0 & 0xffff) as i32, ((lparam.0 >> 16) & 0xffff) as i32);
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
            let dark = current_dark(hwnd);
            if draw_menu_item(lparam, dark) || draw_welcome_button(lparam, dark) {
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_ERASEBKGND => {
            if welcome_visible(hwnd) {
                erase_welcome_background(hwnd, HDC(wparam.0 as *mut _));
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT => {
            if welcome_visible(hwnd) {
                paint_welcome(hwnd);
                return LRESULT(0);
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
            // EN_CHANGE from the editor means the user changed the text; flag the
            // document as having unsaved edits (programmatic updates are skipped
            // via suppress_dirty).
            if ((wparam.0 >> 16) & 0xffff) as u32 == EN_CHANGE {
                let rich = state(hwnd).map_or(HWND(null_mut()), |s| s.borrow().rich_edit);
                if !rich.0.is_null() && lparam.0 == rich.0 as isize {
                    mark_dirty(hwnd);
                    return LRESULT(0);
                }
            }
            match wparam.0 & 0xffff {
                ID_FILE_OPEN => {
                    if let Some(path) = choose_markdown_file(hwnd) {
                        load_markdown(hwnd, &path);
                    }
                }
                ID_FILE_SAVE => {
                    save_document(hwnd);
                }
                ID_FILE_EXIT => {
                    // Go through WM_CLOSE so the unsaved-changes prompt runs.
                    let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                ID_HELP_ABOUT => {
                    show_about(hwnd);
                }
                ID_SETTINGS_DARKMODE => {
                    toggle_dark_mode(hwnd);
                }
                ID_SETTINGS_EDITMODE => {
                    toggle_edit_mode(hwnd);
                }
                ID_WELCOME_OPEN => {
                    // The Open button always opens for viewing.
                    set_edit_mode(hwnd, false);
                    if let Some(path) = choose_markdown_file(hwnd) {
                        load_markdown(hwnd, &path);
                    }
                }
                ID_WELCOME_EDIT => {
                    // The Edit button opens the file directly into edit mode.
                    set_edit_mode(hwnd, true);
                    if let Some(path) = choose_markdown_file(hwnd) {
                        load_markdown(hwnd, &path);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if confirm_discard_changes(hwnd) {
                let _ = DestroyWindow(hwnd);
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
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_SAVE, w!("&Save"));
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_EXIT, w!("E&xit"));
    let _ = AppendMenuW(settings_menu, MF_STRING, ID_SETTINGS_DARKMODE, w!("&Dark Mode"));
    let _ = AppendMenuW(settings_menu, MF_STRING, ID_SETTINGS_EDITMODE, w!("&Edit Mode"));
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
    attach_label(file_menu, ID_FILE_SAVE as u32, false, "&Save", false);
    attach_label(file_menu, ID_FILE_EXIT as u32, false, "E&xit", false);
    attach_label(settings_menu, ID_SETTINGS_DARKMODE as u32, false, "&Dark Mode", false);
    attach_label(settings_menu, ID_SETTINGS_EDITMODE as u32, false, "&Edit Mode", false);
    attach_label(help_menu, ID_HELP_ABOUT as u32, false, "&About", false);
}

unsafe fn show_about(hwnd: HWND) {
    let dark = current_dark(hwnd);
    set_welcome_visible(hwnd, false);
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
    let (has_doc, about_visible, edit_mode) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (
                state.current_file.is_some(),
                state.about_visible,
                state.edit_mode,
            )
        }
        None => return,
    };

    let _ = dark;
    if about_visible {
        show_about(hwnd);
    } else if has_doc {
        // Capture any in-progress edits before re-rendering with new colors.
        if edit_mode {
            sync_source_from_editor(hwnd);
        }
        render_document(hwnd);
    } else {
        show_welcome(hwnd);
    }
}

fn colorref(rgb: (u8, u8, u8)) -> COLORREF {
    let (r, g, b) = rgb;
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

// ---------------------------------------------------------------------------
// Edit mode
// ---------------------------------------------------------------------------

// Show the current document per edit mode: editable raw Markdown source, or the
// rendered read-only view. Always renders from the in-memory source.
unsafe fn render_document(hwnd: HWND) {
    let (source, edit_mode, rich_edit) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (state.source.clone(), state.edit_mode, state.rich_edit)
        }
        None => return,
    };
    if rich_edit.0.is_null() {
        return;
    }

    let dark = current_dark(hwnd);
    if edit_mode {
        SendMessageW(rich_edit, EM_SETREADONLY, WPARAM(0), LPARAM(0));
        let wide = to_wide(&source);
        set_suppress_dirty(hwnd, true);
        let _ = SetWindowTextW(rich_edit, PCWSTR(wide.as_ptr()));
        set_edit_char_format(rich_edit, dark);
        set_suppress_dirty(hwnd, false);
    } else {
        set_rtf(hwnd, markdown_to_rtf(&source, dark));
        SendMessageW(rich_edit, EM_SETREADONLY, WPARAM(1), LPARAM(0));
    }
}

// Apply a monospace, theme-colored character format for editing plain source.
unsafe fn set_edit_char_format(rich_edit: HWND, dark: bool) {
    let mut cf = CHARFORMAT2W::default();
    cf.Base.cbSize = size_of::<CHARFORMAT2W>() as u32;
    cf.Base.dwMask = CFM_MASK(CFM_COLOR.0 | CFM_FACE.0 | CFM_SIZE.0);
    cf.Base.crTextColor = colorref(if dark { MENU_TEXT } else { (24, 24, 27) });
    cf.Base.yHeight = 220; // ~11pt, in twips
    for (slot, ch) in cf.Base.szFaceName.iter_mut().zip("Consolas".encode_utf16()) {
        *slot = ch;
    }
    SendMessageW(
        rich_edit,
        EM_SETCHARFORMAT,
        WPARAM(SCF_ALL as usize),
        LPARAM(&cf as *const CHARFORMAT2W as isize),
    );
}

// Pull the edited text out of the control back into the in-memory source.
unsafe fn sync_source_from_editor(hwnd: HWND) {
    let rich_edit = match state(hwnd) {
        Some(state) => state.borrow().rich_edit,
        None => return,
    };
    if rich_edit.0.is_null() {
        return;
    }

    let len = GetWindowTextLengthW(rich_edit);
    if len <= 0 {
        if let Some(state) = state(hwnd) {
            state.borrow_mut().source.clear();
        }
        return;
    }

    let mut buf = vec![0u16; len as usize + 1];
    let got = GetWindowTextW(rich_edit, &mut buf).max(0) as usize;
    // RichEdit reports line breaks as bare CR; normalize to LF for the parser.
    let text = String::from_utf16_lossy(&buf[..got])
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if let Some(state) = state(hwnd) {
        state.borrow_mut().source = text;
    }
}

// Set the edit-mode flag and reflect it in the menu check mark (no view change).
unsafe fn set_edit_mode(hwnd: HWND, edit: bool) {
    if let Some(state) = state(hwnd) {
        state.borrow_mut().edit_mode = edit;
    }
    let menu = GetMenu(hwnd);
    let check = if edit { MF_CHECKED } else { MF_UNCHECKED };
    CheckMenuItem(menu, ID_SETTINGS_EDITMODE as u32, (MF_BYCOMMAND | check).0);
}

unsafe fn toggle_edit_mode(hwnd: HWND) {
    let (has_doc, was_edit) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (
                state.current_file.is_some() && !state.about_visible,
                state.edit_mode,
            )
        }
        None => return,
    };

    // Capture edits before switching back to the rendered view.
    if has_doc && was_edit {
        sync_source_from_editor(hwnd);
    }
    set_edit_mode(hwnd, !was_edit);

    if has_doc {
        render_document(hwnd);
        let rich_edit = state(hwnd).map_or(HWND(null_mut()), |s| s.borrow().rich_edit);
        if !rich_edit.0.is_null() {
            let _ = SetFocus(rich_edit);
        }
    }
}

unsafe fn is_dirty(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().dirty)
}

// Toggle suppression of edit notifications around programmatic text updates so
// they don't mark the document dirty.
unsafe fn set_suppress_dirty(hwnd: HWND, value: bool) {
    if let Some(state) = state(hwnd) {
        state.borrow_mut().suppress_dirty = value;
    }
}

// Flag the document as having unsaved edits and refresh the title bar. No-op
// while suppressed or when already dirty.
unsafe fn mark_dirty(hwnd: HWND) {
    let changed = match state(hwnd) {
        Some(state) => {
            let mut state = state.borrow_mut();
            if state.suppress_dirty || state.dirty {
                false
            } else {
                state.dirty = true;
                true
            }
        }
        None => false,
    };
    if changed {
        refresh_title(hwnd);
    }
}

// Set the window title to reflect the current document and dirty state.
unsafe fn refresh_title(hwnd: HWND) {
    let (file, dirty, about) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (state.current_file.clone(), state.dirty, state.about_visible)
        }
        None => return,
    };

    let title = if about {
        "Markd - About".to_string()
    } else {
        match file {
            Some(path) => format!("Markd - {}{}", path.display(), if dirty { " *" } else { "" }),
            None => "Markd".to_string(),
        }
    };
    let wide = to_wide(&title);
    let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
}

// Prompt to save when closing with unsaved edits. Returns true if it is OK to
// proceed with closing (saved or discarded), false to cancel and stay open.
unsafe fn confirm_discard_changes(hwnd: HWND) -> bool {
    let (dirty, has_file, about) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (state.dirty, state.current_file.is_some(), state.about_visible)
        }
        None => return true,
    };
    if !dirty || !has_file || about {
        return true;
    }

    let result = MessageBoxW(
        hwnd,
        w!("You have unsaved changes. Save before closing?"),
        APP_TITLE,
        MB_YESNOCANCEL | MB_ICONWARNING,
    );
    if result == IDYES {
        save_document(hwnd);
        // If the save failed or its dialog was cancelled the document is still
        // dirty; keep the window open.
        !is_dirty(hwnd)
    } else if result == IDNO {
        true
    } else {
        // IDCANCEL or the dialog was dismissed.
        false
    }
}

// Write the current source back to disk. Prompts for a path if the document has
// no associated file. No-op when there is no open document.
unsafe fn save_document(hwnd: HWND) {
    let (has_doc, about, edit_mode, current_file) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (
                state.current_file.is_some(),
                state.about_visible,
                state.edit_mode,
                state.current_file.clone(),
            )
        }
        None => return,
    };
    if about || !has_doc {
        return;
    }

    // Make sure in-progress edits are captured before writing.
    if edit_mode {
        sync_source_from_editor(hwnd);
    }

    let target = match current_file.or_else(|| choose_save_path(hwnd)) {
        Some(path) => path,
        None => return,
    };

    let source = state(hwnd).map_or(String::new(), |s| s.borrow().source.clone());
    match fs::write(&target, source) {
        Ok(()) => {
            if let Some(state) = state(hwnd) {
                let mut state = state.borrow_mut();
                state.current_file = Some(target.clone());
                state.dirty = false;
            }
            refresh_title(hwnd);
        }
        Err(error) => {
            let message = format!("Could not save file:\n{}\n\n{}", target.display(), error);
            let wide = to_wide(&message);
            let _ = MessageBoxW(
                hwnd,
                PCWSTR(wide.as_ptr()),
                w!("Markd"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}

// Save As dialog, returning the chosen path. Defaults to a .md extension.
unsafe fn choose_save_path(hwnd: HWND) -> Option<PathBuf> {
    let mut file_name = [0u16; MAX_PATH as usize];
    let filter = OPEN_FILTER.with(|filter| filter.as_ptr());
    let default_ext = to_wide("md");
    let mut save = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter),
        lpstrFile: PWSTR(file_name.as_mut_ptr()),
        nMaxFile: file_name.len() as u32,
        lpstrDefExt: PCWSTR(default_ext.as_ptr()),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY,
        ..Default::default()
    };

    if GetSaveFileNameW(&mut save).as_bool() {
        let len = file_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(file_name.len());
        Some(PathBuf::from(String::from_utf16_lossy(&file_name[..len])))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Welcome (start) screen
// ---------------------------------------------------------------------------

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

unsafe fn welcome_visible(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().welcome_visible)
}

// Create the two owner-drawn buttons and load the display icon. Called once
// from WM_CREATE; the controls start hidden and are shown via the welcome view.
unsafe fn create_welcome_controls(hwnd: HWND) {
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
unsafe fn layout_welcome(hwnd: HWND, width: i32, height: i32) {
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

unsafe fn show_welcome(hwnd: HWND) {
    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.current_file = None;
        state.about_visible = false;
    }
    set_welcome_visible(hwnd, true);
    let _ = SetWindowTextW(hwnd, APP_TITLE);
}

// Toggle the welcome controls and the document view (they are mutually
// exclusive: the welcome screen owns the client area, otherwise the RichEdit
// does).
unsafe fn set_welcome_visible(hwnd: HWND, visible: bool) {
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

unsafe fn erase_welcome_background(hwnd: HWND, hdc: HDC) {
    let mut rc = RECT::default();
    if GetClientRect(hwnd, &mut rc).is_err() {
        return;
    }
    let brush = CreateSolidBrush(colorref(welcome_bg(current_dark(hwnd))));
    fill_rect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

// Paint the left pane: app icon and name (background already erased).
unsafe fn paint_welcome(hwnd: HWND) {
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
unsafe fn draw_welcome_button(lparam: LPARAM, dark: bool) -> bool {
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
    let content = strip_mnemonic(&label.text);
    let content = trim_null(&content);
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

// Tab separates a menu label from its accelerator hint (e.g. "Save").
const TAB: u16 = 9;

// Slice off a trailing null terminator if present.
fn trim_null(text: &[u16]) -> &[u16] {
    match text.split_last() {
        Some((&0, rest)) => rest,
        _ => text,
    }
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
        Ok(markdown) => unsafe {
            set_welcome_visible(hwnd, false);
            let mut rich_edit = HWND(null_mut());
            if let Some(state) = state(hwnd) {
                let mut state = state.borrow_mut();
                state.source = markdown;
                state.current_file = Some(path.to_path_buf());
                state.about_visible = false;
                state.dirty = false;
                rich_edit = state.rich_edit;
            }
            render_document(hwnd);
            if !rich_edit.0.is_null() {
                let _ = SetFocus(rich_edit);
            }
            refresh_title(hwnd);
        },
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

        set_suppress_dirty(hwnd, true);
        SendMessageW(
            rich_edit,
            EM_STREAMIN,
            WPARAM(SF_RTF as usize),
            LPARAM((&mut edit_stream as *mut EDITSTREAM) as isize),
        );
        set_suppress_dirty(hwnd, false);
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

fn about_rtf(dark: bool) -> String {
    format!(
        r#"{}\pard\cf1\f0\fs40\b Markd\b0\par\pard\sa240\fs22 Lightweight native Markdown viewer and editor for Windows, built with Rust for speed, simplicity, and efficiency.\par\pard\sa140\b Author:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://khalidutsob.com"}}}}{{\fldrslt{{\cf2\ul Khalid Utsob}}}}}}\ul0\cf1\par\pard\sa140\b GitHub:\b0  {{\field{{\*\fldinst{{HYPERLINK "https://github.com/en-arnob/markd"}}}}{{\fldrslt{{\cf2\ul en-arnob/markd}}}}}}\ul0\cf1\par\pard\sa140\b Version:\b0  1.0.3\par}}"#,
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
