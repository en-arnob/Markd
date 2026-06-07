# Markd

Markd is a lightweight native Windows Markdown viewer written in Rust. It uses
Win32 for the application shell, the native RichEdit control for display, and
`pulldown-cmark` for Markdown parsing.

## Features

- Native Windows window, menu bar, file dialog, and RichEdit viewer
- Open Markdown files from the menu or by passing a file path on the command line
- Basic Markdown rendering for headings, emphasis, strong text, links, lists,
  quotes, code, code blocks, rules, and tables as readable text
- No web runtime or embedded browser

## Build

Install Rust, then run:

```powershell
cargo build --release
```

The executable will be written to:

```text
target\release\markd.exe
```

## Run

```powershell
cargo run -- README.md
```

Or start the app and choose `File > Open`.
