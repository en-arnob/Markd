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

## Installer

A single standalone Windows installer is produced with one command:

```powershell
cargo dist
```

This builds the release binary and packages it into a single-file installer
([Inno Setup](https://jrsoftware.org/isinfo.php)) at:

```text
dist\Markd-Setup-<version>.exe
```

`cargo dist` is a [cargo xtask](xtask/src/main.rs): it runs `cargo build
--release`, generates `installer\markd.ico` from `src\assets\icon.png`, installs
Inno Setup via `winget` if it isn't already present, and compiles
[installer\markd.iss](installer/markd.iss).

The installer adds Start Menu (and optional desktop) shortcuts and registers
Markd as a handler for Markdown files (`.md`, `.markdown`, `.mdown`, `.mkd`) —
including a "Default Apps" entry so Windows can set it as the default.

Notes:

- The app binary is statically linked (`+crt-static`), so the installed
  `markd.exe` is standalone and needs no Visual C++ redistributable.
- Run without administrator rights the installer installs per-user (to
  `%LOCALAPPDATA%\Programs\Markd`); run elevated it installs per-machine to
  `Program Files`. File associations follow the same scope.
- The "Set Markd as the default app for Markdown files" checkbox is on by
  default; uncheck it to only add Markd to the *Open with* list.
- The installer is unsigned, so SmartScreen may warn on first run.
