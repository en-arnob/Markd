//! Everything about the document view: rendering Markdown (or raw source in edit
//! mode), the About/Learn pages, dirty tracking and the title bar, loading and
//! saving files, RTF streaming into the RichEdit, and hyperlink handling.

use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr::{copy_nonoverlapping, null_mut};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, MAX_PATH, RECT, WPARAM};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Controls::RichEdit::{
    CFM_COLOR, CFM_FACE, CFM_MASK, CFM_SIZE, CHARFORMAT2W, CHARRANGE, EDITSTREAM, EM_GETTEXTRANGE,
    EM_SETCHARFORMAT, EM_STREAMIN, ENLINK, EN_LINK, SCF_ALL, SF_RTF, TEXTRANGEW,
};
use windows::Win32::UI::Controls::{EM_SETREADONLY, EM_SETRECT, NMHDR};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CheckMenuItem, GetMenu, GetWindowTextLengthW, GetWindowTextW, MessageBoxW, SendMessageW,
    SetWindowTextW, IDNO, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_YESNOCANCEL, MF_BYCOMMAND,
    MF_CHECKED, MF_UNCHECKED, WM_LBUTTONDOWN,
};

use crate::app::{
    state, APP_TITLE, ID_SETTINGS_EDITMODE, LEARN_MARKDOWN, MENU_TEXT, VIEW_PADDING,
};
use crate::markdown::{about_rtf, markdown_to_rtf};
use crate::theme::current_dark;
use crate::util::{colorref, open_url, to_wide, wide_filter};
use crate::welcome::set_welcome_visible;

thread_local! {
    static OPEN_FILTER: Vec<u16> = wide_filter(&[
        ("Markdown files", "*.md;*.markdown;*.mdown;*.mkd"),
        ("Text files", "*.txt"),
        ("All files", "*.*"),
    ]);
}

// Backing buffer for an EM_STREAMIN RTF stream.
struct RtfStream {
    data: Vec<u8>,
    position: usize,
}

// ---------------------------------------------------------------------------
// Special pages (About / Learn)
// ---------------------------------------------------------------------------

pub(crate) unsafe fn show_about(hwnd: HWND) {
    let dark = current_dark(hwnd);
    set_welcome_visible(hwnd, false);
    set_rtf(hwnd, about_rtf(dark));
    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.current_file = None;
        state.about_visible = true;
        state.learn_visible = false;
    }
    let _ = SetWindowTextW(hwnd, w!("Markd - About"));
    // Move focus off the read-only RichEdit so it stops showing a blinking
    // caret on the About page. Links remain clickable without focus.
    let _ = SetFocus(hwnd);
}

// Show the built-in Markdown tutorial in the rendered (read-only) view. It is
// not tied to a file, so it can't be saved over and Ctrl+S is a no-op here.
pub(crate) unsafe fn show_learn(hwnd: HWND) {
    set_welcome_visible(hwnd, false);
    if let Some(state) = state(hwnd) {
        let mut state = state.borrow_mut();
        state.source = LEARN_MARKDOWN.to_string();
        state.current_file = None;
        state.about_visible = false;
        state.learn_visible = true;
        state.dirty = false;
    }
    // Always present the tutorial rendered, never as raw source.
    set_edit_mode(hwnd, false);
    render_document(hwnd);
    let _ = SetWindowTextW(hwnd, w!("Markd - Learn Markdown"));
    // Keep focus off the read-only view so no caret blinks; links stay clickable.
    let _ = SetFocus(hwnd);
}

// ---------------------------------------------------------------------------
// Rendering & edit mode
// ---------------------------------------------------------------------------

// Show the current document per edit mode: editable raw Markdown source, or the
// rendered read-only view. Always renders from the in-memory source.
pub(crate) unsafe fn render_document(hwnd: HWND) {
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
pub(crate) unsafe fn sync_source_from_editor(hwnd: HWND) {
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
pub(crate) unsafe fn set_edit_mode(hwnd: HWND, edit: bool) {
    if let Some(state) = state(hwnd) {
        state.borrow_mut().edit_mode = edit;
    }
    let menu = GetMenu(hwnd);
    let check = if edit { MF_CHECKED } else { MF_UNCHECKED };
    CheckMenuItem(menu, ID_SETTINGS_EDITMODE as u32, (MF_BYCOMMAND | check).0);
}

pub(crate) unsafe fn toggle_edit_mode(hwnd: HWND) {
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

// ---------------------------------------------------------------------------
// Dirty tracking & title bar
// ---------------------------------------------------------------------------

pub(crate) unsafe fn is_dirty(hwnd: HWND) -> bool {
    state(hwnd).map_or(false, |state| state.borrow().dirty)
}

// Toggle suppression of edit notifications around programmatic text updates so
// they don't mark the document dirty.
pub(crate) unsafe fn set_suppress_dirty(hwnd: HWND, value: bool) {
    if let Some(state) = state(hwnd) {
        state.borrow_mut().suppress_dirty = value;
    }
}

// Flag the document as having unsaved edits and refresh the title bar. No-op
// while suppressed or when already dirty.
pub(crate) unsafe fn mark_dirty(hwnd: HWND) {
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
pub(crate) unsafe fn refresh_title(hwnd: HWND) {
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
pub(crate) unsafe fn confirm_discard_changes(hwnd: HWND) -> bool {
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

// ---------------------------------------------------------------------------
// File open / save
// ---------------------------------------------------------------------------

// Write the current source back to disk. Prompts for a path if the document has
// no associated file. No-op when there is no open document.
pub(crate) unsafe fn save_document(hwnd: HWND) {
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
            let _ = MessageBoxW(hwnd, PCWSTR(wide.as_ptr()), w!("Markd"), MB_ICONERROR);
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

pub(crate) unsafe fn choose_markdown_file(hwnd: HWND) -> Option<PathBuf> {
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

pub(crate) fn load_markdown(hwnd: HWND, path: &Path) {
    match fs::read_to_string(path) {
        Ok(markdown) => unsafe {
            set_welcome_visible(hwnd, false);
            let mut rich_edit = HWND(null_mut());
            if let Some(state) = state(hwnd) {
                let mut state = state.borrow_mut();
                state.source = markdown;
                state.current_file = Some(path.to_path_buf());
                state.about_visible = false;
                state.learn_visible = false;
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
            let _ = MessageBoxW(hwnd, PCWSTR(wide_message.as_ptr()), w!("Markd"), MB_ICONERROR);
        },
    }
}

// ---------------------------------------------------------------------------
// RTF streaming
// ---------------------------------------------------------------------------

pub(crate) unsafe fn set_rtf(hwnd: HWND, rtf: String) {
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

// ---------------------------------------------------------------------------
// Layout & links
// ---------------------------------------------------------------------------

pub(crate) unsafe fn set_view_padding(rich_edit: HWND, width: i32, height: i32) {
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

pub(crate) unsafe fn handle_notification(hwnd: HWND, lparam: LPARAM) {
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
