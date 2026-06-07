//! Window class registration and the main window procedure that dispatches
//! Win32 messages to the feature modules.

use std::cell::RefCell;
use std::ptr::null_mut;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, HDC, WHITE_BRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::RichEdit::{EM_SETEVENTMASK, ENM_CHANGE, ENM_LINK};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, LoadCursorW, LoadIconW,
    MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor,
    SetWindowLongPtrW, CREATESTRUCTW, EN_CHANGE, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY,
    GWLP_USERDATA, HICON, IDC_ARROW, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
    WM_DRAWITEM, WM_ERASEBKGND, WM_MEASUREITEM, WM_MENUCHAR, WM_NCCREATE, WM_NCPAINT, WM_NOTIFY,
    WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE, WS_VISIBLE, WS_VSCROLL,
};

use crate::app::{
    state, AppState, APP_CLASS, ID_FILE_EXIT, ID_FILE_OPEN, ID_FILE_SAVE, ID_HELP_ABOUT,
    ID_LEARN_MARKDOWN, ID_SETTINGS_DARKMODE, ID_SETTINGS_EDITMODE, ID_WELCOME_EDIT, ID_WELCOME_OPEN,
};
use crate::menu::{create_menu, draw_menu_item, handle_menu_char, measure_menu_item, paint_menu_bar_background};
use crate::theme::{current_dark, toggle_dark_mode};
use crate::view::{
    choose_markdown_file, confirm_discard_changes, handle_notification, load_markdown, mark_dirty,
    save_document, set_edit_mode, set_view_padding, show_about, show_learn, toggle_edit_mode,
};
use crate::welcome::{
    create_welcome_controls, draw_welcome_button, erase_welcome_background, layout_welcome,
    paint_welcome, welcome_visible,
};

pub(crate) unsafe fn register_window_class(instance: HINSTANCE) -> windows::core::Result<()> {
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
                ID_LEARN_MARKDOWN => {
                    show_learn(hwnd);
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
