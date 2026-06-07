//! Shared application state and the constants used across the app.

use std::cell::RefCell;
use std::path::PathBuf;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA, HICON};

// Window class / title.
pub(crate) const APP_CLASS: PCWSTR = w!("MarkdWindow");
pub(crate) const APP_TITLE: PCWSTR = w!("Markd");

// Menu command identifiers.
pub(crate) const ID_FILE_OPEN: usize = 1001;
pub(crate) const ID_FILE_EXIT: usize = 1002;
pub(crate) const ID_FILE_SAVE: usize = 1003;
pub(crate) const ID_HELP_ABOUT: usize = 2001;
pub(crate) const ID_LEARN_MARKDOWN: usize = 2002;
pub(crate) const ID_SETTINGS_DARKMODE: usize = 3001;
pub(crate) const ID_SETTINGS_EDITMODE: usize = 3002;
pub(crate) const ID_WELCOME_OPEN: usize = 4001;
pub(crate) const ID_WELCOME_EDIT: usize = 4002;

// Padding (logical px) around the rendered document inside the RichEdit.
pub(crate) const VIEW_PADDING: i32 = 24;

// Built-in Markdown tutorial shown via Help -> Learn -> Markdown Basics.
pub(crate) const LEARN_MARKDOWN: &str = include_str!("../assets/learn-markdown.md");

// Welcome screen layout (logical pixels).
pub(crate) const WELCOME_ICON: i32 = 96;
pub(crate) const WELCOME_BTN_W: i32 = 200;
pub(crate) const WELCOME_BTN_H: i32 = 48;
pub(crate) const WELCOME_BTN_GAP: i32 = 16;

// RichEdit background colors (COLORREF, 0x00BBGGRR).
pub(crate) const LIGHT_BG: isize = 0x00FF_FFFF; // white
pub(crate) const DARK_BG: isize = 0x001E_1E1E; // #1e1e1e

// Dark menu palette (R, G, B).
pub(crate) const MENU_BAR_BG: (u8, u8, u8) = (43, 43, 43); // #2b2b2b
pub(crate) const MENU_HOT_BG: (u8, u8, u8) = (60, 60, 60); // #3c3c3c
pub(crate) const MENU_TEXT: (u8, u8, u8) = (220, 220, 220); // #dcdcdc
pub(crate) const MENU_TEXT_DISABLED: (u8, u8, u8) = (120, 120, 120);

// Tab separates a menu label from its accelerator hint (e.g. "Save\tCtrl+S").
pub(crate) const TAB: u16 = 9;

pub(crate) struct AppState {
    pub(crate) rich_edit: HWND,
    pub(crate) current_file: Option<PathBuf>,
    pub(crate) about_visible: bool,
    // True while the built-in Markdown tutorial is on screen (Help -> Learn).
    pub(crate) learn_visible: bool,
    pub(crate) dark_mode: bool,
    // Edit mode shows the raw Markdown source as editable text; otherwise the
    // rendered document is shown read-only. `source` holds the current Markdown
    // in memory so edits survive theme/mode toggles.
    pub(crate) edit_mode: bool,
    pub(crate) source: String,
    // True when the in-memory source has unsaved edits. Reflected in the title
    // bar (a trailing " *") and used to prompt before closing.
    pub(crate) dirty: bool,
    // Set while we update the editor programmatically (loading/rendering) so the
    // resulting EN_CHANGE notifications don't get mistaken for user edits.
    pub(crate) suppress_dirty: bool,
    // Welcome (start) screen: shown when no document is open.
    pub(crate) welcome_visible: bool,
    pub(crate) welcome_open: HWND,
    pub(crate) welcome_edit: HWND,
    pub(crate) welcome_icon: HICON,
}

// Borrow the per-window `AppState` stashed in GWLP_USERDATA at window creation.
pub(crate) unsafe fn state(hwnd: HWND) -> Option<&'static RefCell<AppState>> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<AppState>;
    ptr.as_ref()
}
