# Marky

A tiny markdown viewer for opening local `.md` files fast, reading them cleanly, and dropping into source when you need to edit.

> Marky is intentionally small. It is a desktop utility, not a workspace, sync service, or note-taking platform.

## Highlights

- Opens local markdown files directly from the app, drag and drop, or OS file associations
- Renders sanitized markdown in a simple desktop window with syntax-highlighted fenced code blocks
- Switches between rendered view and raw source with quick editing and save prompts
- Opens folders of markdown files with a sidebar and predictable first-file selection
- Builds for Windows locally and can now be built for macOS from a Mac

## Overview

Marky is a Tauri app with a plain HTML/CSS/JS frontend and a Rust backend. The Rust side handles markdown loading, folder scanning, sanitization, and OS integration. The frontend stays simple and focuses on presentation, file switching, and edit state.

The project is optimized for people who just want to open markdown files on their machine without pulling in a full editor. It is currently Windows-first in its release posture, but the build setup now also supports macOS packaging from a Mac.

### Author

Built by [@slainhedden](https://github.com/slainhedden).

## Usage

Open a file and read it:

```text
File -> Open File
```

Open a folder of docs and browse the markdown files from the sidebar:

```text
File -> Open Folder
```

Edit raw source, then save with:

```text
Ctrl+S
```

Marky also supports:

- Drag and drop
- Opening a markdown file from the command line or file association
- Theme cycling
- Rendered/source view toggling
- Prompts before discarding unsaved changes

## Installation

There is not a package-manager install flow yet. For now, use a built desktop artifact:

- Windows: run the NSIS installer
- macOS: open the generated `.dmg` and move `Marky.app` into `Applications`

Supported targets:

- Windows
- macOS, built on macOS

## Build From Source

Install dependencies:

```bash
npm install
```

Start the app in development mode:

```bash
npm run dev
```

If you are launching from WSL and want the Windows-side toolchain explicitly:

```bash
cmd.exe /c npm run dev
```

Build the Windows release executable and NSIS installer:

```bash
npm run build
```

Or explicitly:

```bash
npm run build:windows
```

From WSL:

```bash
cmd.exe /c npm run build
```

Typical Windows build outputs:

- `src-tauri/target/release/barebones-markdown-viewer.exe`
- `src-tauri/target/release/bundle/nsis/Marky_0.1.0_x64-setup.exe`

Build the macOS app bundle and DMG from a Mac:

```bash
npm run build:mac
```

Typical macOS build outputs:

- `src-tauri/target/release/bundle/macos/Marky.app`
- `src-tauri/target/release/bundle/dmg/Marky_0.1.0_aarch64.dmg`
- `src-tauri/target/release/bundle/dmg/Marky_0.1.0_x64.dmg`

## Set Marky As Default For `.md` On Windows

1. Run the installer.
2. Right-click any `.md` file in Explorer.
3. Choose `Open with`.
4. Choose `Marky`.
5. Turn on `Always use this app to open .md files`.

The installer is configured to register markdown file associations for:

- `.md`
- `.markdown`
- `.mdown`
- `.mkd`

## Notes

- The frontend lives in `dist/` and stays framework-free.
- Markdown is rendered and sanitized in Rust before it reaches the UI.
- Source mode is intentionally a simple textarea, not a full editor.
- Folder mode scans once when you open a folder, then reuses the file list for navigation.

## Feedback and Contributing

Open an issue if you hit a bug or want a focused feature. If you want to contribute, keep changes aligned with the project’s core constraint: a very small, skimmable markdown viewer with minimal dependencies.
