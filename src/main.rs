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
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, MAX_PATH, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, UpdateWindow, HBRUSH, WHITE_BRUSH};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Controls::RichEdit::{
    CHARRANGE, EDITSTREAM, EM_GETTEXTRANGE, EM_SETEVENTMASK, EM_STREAMIN, ENLINK, ENM_LINK,
    EN_LINK, SF_RTF, TEXTRANGEW,
};
use windows::Win32::UI::Controls::EM_SETRECT;
use windows::Win32::UI::Controls::NMHDR;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateMenu, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, LoadCursorW, MessageBoxW, MoveWindow, PostQuitMessage,
    RegisterClassW, SendMessageW, SetCursor, SetMenu, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, ES_AUTOVSCROLL, ES_MULTILINE,
    ES_READONLY, GWLP_USERDATA, IDC_ARROW, MF_POPUP, MF_STRING, MSG, SW_SHOW, SW_SHOWNORMAL,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDOWN, WM_NCCREATE,
    WM_NOTIFY, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_CHILD, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE, WS_VSCROLL,
};

const APP_CLASS: PCWSTR = w!("MarkdWindow");
const APP_TITLE: PCWSTR = w!("Markd");
const ID_FILE_OPEN: usize = 1001;
const ID_FILE_EXIT: usize = 1002;
const ID_HELP_ABOUT: usize = 2001;
const VIEW_PADDING: i32 = 24;

struct AppState {
    rich_edit: HWND,
    current_file: Option<PathBuf>,
    about_visible: bool,
}

struct RtfStream {
    data: Vec<u8>,
    position: usize,
}

thread_local! {
    static OPEN_FILTER: Vec<u16> = wide_filter(&[
        ("Markdown files", "*.md;*.markdown;*.mdown;*.mkd"),
        ("Text files", "*.txt"),
        ("All files", "*.*"),
    ]);
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
            set_rtf(hwnd, welcome_rtf());
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
    let class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW)?,
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
    let help_menu = CreateMenu().unwrap_or_default();

    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_OPEN, w!("&Open..."));
    let _ = AppendMenuW(file_menu, MF_STRING, ID_FILE_EXIT, w!("E&xit"));
    let _ = AppendMenuW(help_menu, MF_STRING, ID_HELP_ABOUT, w!("&About"));
    let _ = AppendMenuW(menu, MF_POPUP, file_menu.0 as usize, w!("&File"));
    let _ = AppendMenuW(menu, MF_POPUP, help_menu.0 as usize, w!("&Help"));
    let _ = SetMenu(hwnd, menu);
}

unsafe fn show_about(hwnd: HWND) {
    set_rtf(hwnd, about_rtf());
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
            let rtf = markdown_to_rtf(&markdown);
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
    let stream = &mut *(c
        ookie as *mut RtfStream);
    let remaining = stream.data.len().saturating_sub(stream.position);
    let count = remaining.min(buffer_len as usize);

    if count > 0 {
        copy_nonoverlapping(stream.data.as_ptr().add(stream.position), buffer, count);
        stream.position += count;
    }

    *bytes_written = count as i32;
    0
}

fn welcome_rtf() -> String {
    r"{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}{\f1 Consolas;}}{\colortbl;\red24\green24\blue27;\red101\green117\blue133;\red246\green248\blue250;}\paperw12240\paperh15840\margl720\margr720\viewkind4\uc1\pard\sa220\f0\fs36\b Markd\b0\par\fs22 Open a Markdown file with File > Open, or pass a path on the command line.\par}".to_string()
}

fn about_rtf() -> String {
    r#"{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}{\f1 Consolas;}}{\colortbl;\red24\green24\blue27;\red101\green117\blue133;\red246\green248\blue250;}\paperw12240\paperh15840\margl720\margr720\viewkind4\uc1\pard\cf1\f0\fs40\b Markd\b0\par\pard\sa240\fs22 Lightweight native Markdown viewer for Windows, built with Rust for speed, simplicity, and efficiency.\par\pard\sa140\b Author:\b0  {\field{\*\fldinst{HYPERLINK "https://khalidutsob.com"}}{\fldrslt{\cf2\ul Khalid Utsob}}}\ul0\cf1\par\pard\sa140\b GitHub:\b0  {\field{\*\fldinst{HYPERLINK "https://github.com/en-arnob/markd"}}{\fldrslt{\cf2\ul en-arnob/markd}}}\ul0\cf1\par\pard\sa140\b Version:\b0  1.0.3\par}"#.to_string()
}

fn markdown_to_rtf(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut out = String::from(
        r"{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}{\f1 Consolas;}}{\colortbl;\red24\green24\blue27;\red101\green117\blue133;\red246\green248\blue250;}\paperw12240\paperh15840\margl720\margr720\viewkind4\uc1\pard\cf1\f0\fs22 ",
    );
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
