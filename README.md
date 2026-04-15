# Marky

A tiny Windows-first markdown viewer built to do one job well: open local `.md` files quickly, render them cleanly, and let you switch to raw source when you want.

## What it does

- Opens local markdown files from the file picker
- Opens a folder and picks `README*`, then `index*`, then the first markdown file
- Shows a left file tree for folder opens so you can switch between markdown files
- Opens files passed on launch, including installer-based file associations
- Supports drag and drop
- Opens into rendered view every time, with one button to toggle source
- Lets you edit raw source and save it back to disk with `Ctrl+S` or the `Save` button
- Keeps `Open File`, `Open Folder`, `Save`, theme, and view toggle controls available in the app toolbar
- Prompts before discarding unsaved changes when you switch files, follow local links, or close the window
- Cycles between built-in themes and remembers the last one you picked
- Renders headings, lists, quotes, links, code blocks, and tables
- Adds syntax highlighting for fenced code blocks with plain-text fallback for unknown languages
- Renders `diff` and `patch` fences with added and removed lines tinted clearly
- Uses a fixed dark, GitHub-like reading style

## Run

Install dependencies:

```bash
npm install
```

Start the app in development mode:

```bash
npm run dev
```

If you are launching from WSL and want to force the Windows-side toolchain:

```bash
cmd.exe /c npm run dev
```

## Build

Build the Windows release executable and NSIS installer:

```bash
npm run build
```

From WSL, this also works explicitly:

```bash
cmd.exe /c npm run build
```

Windows build outputs:

- Release exe: `src-tauri/target/release/barebones-markdown-viewer.exe`
- Windows installer: `src-tauri/target/release/bundle/nsis/Marky_0.1.0_x64-setup.exe`

The installer uses Tauri's default WebView2 bootstrapper flow, which keeps the installer small and uses the system WebView2 runtime on Windows.

Build the macOS app bundle and DMG from a Mac:

```bash
npm run build:mac
```

Typical macOS build outputs:

- App bundle: `src-tauri/target/release/bundle/macos/Marky.app`
- Disk image: `src-tauri/target/release/bundle/dmg/Marky_0.1.0_aarch64.dmg` or `Marky_0.1.0_x64.dmg`

## Set As Default For `.md` On Windows

1. Run the installer.
2. Right-click any `.md` file in Explorer.
3. Choose `Open with`.
4. Choose `Marky`.
5. Turn on `Always use this app to open .md files`.

You can also set it from Windows Settings:

1. Open `Settings`.
2. Go to `Apps` > `Default apps`.
3. Search for `.md`.
4. Select `Marky` for that file type.

The installer is configured to register markdown file associations for:

- `.md`
- `.markdown`
- `.mdown`
- `.mkd`

## Notes

- The frontend is static HTML/CSS/JS in `dist/`.
- Markdown is rendered in Rust and sanitized before it reaches the UI.
- Syntax highlighting stays backend-rendered and theme-aware. The frontend still just displays sanitized HTML.
- Source mode is still intentionally plain. It is a quick-edit textarea, not a full editor.
- Folder mode scans once when you open a folder, then reuses the cached sidebar list for file switches.
