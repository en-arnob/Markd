#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Markd — a lightweight native Windows Markdown viewer/editor.
//!
//! Modules:
//! - [`app`]      shared state and constants
//! - [`util`]     small string/rect/GDI helpers
//! - [`markdown`] Markdown -> RTF conversion
//! - [`menu`]     menu bar + owner-draw
//! - [`welcome`]  welcome (start) screen
//! - [`theme`]    dark/light theming
//! - [`view`]     document view, editing, load/save, links
//! - [`window`]   window class + message loop dispatch

mod app;
mod markdown;
mod menu;
mod theme;
mod util;
mod view;
mod welcome;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::null_mut;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateAcceleratorTableW, CreateWindowExW, DispatchMessageW, GetMessageW, ShowWindow,
    TranslateAcceleratorW, TranslateMessage, ACCEL, ACCEL_VIRT_FLAGS, CW_USEDEFAULT, FCONTROL,
    FVIRTKEY, HACCEL, HICON, MSG, SW_SHOW, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use app::{AppState, APP_CLASS, APP_TITLE, ID_FILE_SAVE};
use view::load_markdown;
use welcome::show_welcome;
use window::register_window_class;

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
            learn_visible: false,
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
