//! Small, generic helpers shared across modules (string/rect/GDI utilities).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{FillRect, HBRUSH, HDC};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

// Convert a Rust string to a null-terminated UTF-16 buffer for Win32 APIs.
pub(crate) fn to_wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

// Build a double-null-terminated filter string for the common file dialogs.
pub(crate) fn wide_filter(filters: &[(&str, &str)]) -> Vec<u16> {
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

// Pack an (R, G, B) tuple into a Win32 COLORREF (0x00BBGGRR).
pub(crate) fn colorref(rgb: (u8, u8, u8)) -> COLORREF {
    let (r, g, b) = rgb;
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

// Slice off a trailing null terminator if present.
pub(crate) fn trim_null(text: &[u16]) -> &[u16] {
    match text.split_last() {
        Some((&0, rest)) => rest,
        _ => text,
    }
}

// The character following '&' in a label, lowercased.
pub(crate) fn mnemonic_char(text: &[u16]) -> Option<char> {
    let amp = u16::from(b'&');
    let pos = text.iter().position(|&c| c == amp)?;
    let next = *text.get(pos + 1)?;
    char::from_u32(next as u32).map(|c| c.to_ascii_lowercase())
}

// Drop the first '&' mnemonic marker (it's drawn as an underline, not a glyph)
// so the label measures at its visible width. A trailing null is preserved.
pub(crate) fn strip_mnemonic(text: &[u16]) -> Vec<u16> {
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

// Translate a rect into coordinates relative to `origin`.
pub(crate) fn offset_rect(rc: RECT, origin: POINT) -> RECT {
    RECT {
        left: rc.left - origin.x,
        top: rc.top - origin.y,
        right: rc.right - origin.x,
        bottom: rc.bottom - origin.y,
    }
}

pub(crate) unsafe fn fill_rect(hdc: HDC, rc: &RECT, brush: HBRUSH) {
    FillRect(hdc, rc, brush);
}

// Open a URL in the user's default browser.
pub(crate) unsafe fn open_url(hwnd: HWND, url: &str) {
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
