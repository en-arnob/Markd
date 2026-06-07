//! Dark/light theme: toggling it and re-applying it to the frame, menu,
//! editor background, and current view.

use std::mem::size_of;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::UI::Controls::RichEdit::EM_SETBKGNDCOLOR;
use windows::Win32::UI::WindowsAndMessaging::{
    CheckMenuItem, DrawMenuBar, GetMenu, SendMessageW, MF_BYCOMMAND, MF_CHECKED, MF_UNCHECKED,
};

use crate::app::{state, DARK_BG, ID_SETTINGS_DARKMODE, LIGHT_BG};
use crate::view::{render_document, show_about, show_learn, sync_source_from_editor};
use crate::welcome::show_welcome;

pub(crate) unsafe fn current_dark(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().dark_mode)
}

pub(crate) unsafe fn toggle_dark_mode(hwnd: HWND) {
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
pub(crate) unsafe fn refresh_view(hwnd: HWND, dark: bool) {
    let (has_doc, about_visible, learn_visible, edit_mode) = match state(hwnd) {
        Some(state) => {
            let state = state.borrow();
            (
                state.current_file.is_some(),
                state.about_visible,
                state.learn_visible,
                state.edit_mode,
            )
        }
        None => return,
    };

    let _ = dark;
    if about_visible {
        show_about(hwnd);
    } else if learn_visible {
        show_learn(hwnd);
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
