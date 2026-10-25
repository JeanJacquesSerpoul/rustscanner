# RustScanner

*[Version française](README.md)*

A document scanning application for **Linux and Windows** in the spirit of **Skanpage** (KDE), written entirely in Rust: **Tauri 2** core, **Leptos** (WebAssembly) interface. **French and English** interface, light / dark theme.

## Features

- **Every SANE** (Linux) **or WIA** (Windows) **scanner**: USB, network (eSCL / AirScan, saned, WSD), automatic discovery, reopens the last scanner used.
- **Essential settings at a glance**: source (flatbed, feeder, duplex), color / grayscale / black & white, resolution chips with usage hint and estimated size, page sizes (A4, A5, Letter, Legal, photo, card…) with a portrait / landscape toggle.
- **Every scanner option** generated automatically (sliders, switches, lists), titles translated with the sane-backends catalog, values remembered per scanner, factory reset.
- **Preview and scan area**: low-resolution preview of the whole glass, a frame to draw, move and resize with the mouse (8 handles), size in millimetres and recognized paper size.
- **Multi-page documents**: batch scanning from the feeder, thumbnails with multi-selection (Ctrl / Shift), drag and drop to reorder, rotate, turn, mirror, delete with **Undo**.
- **Viewer**: fit page, fit width, actual size (1:1), Ctrl + wheel zoom.
- **Saving**: multi-page PDF at true physical size (line-art pages stored losslessly as 1-bit), PNG, JPEG (dpi density), TIFF, lossless WebP; file name pattern (`{date}`, `{time}`, `{n}`) that never overwrites; open the file or its folder from the notification.
- **Printing** through CUPS, or Windows printers (printer, copies, selected pages).
- **Safety**: confirmation before quitting or clearing an unsaved document, cancel a scan at any time (Esc), temporary files cleaned up.

## Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Scan / preview | `Ctrl+Enter` / `Ctrl+Shift+Enter` |
| Cancel the scan | `Esc` |
| Save / print / new | `Ctrl+S` / `Ctrl+P` / `Ctrl+N` |
| Previous / next page | arrows, `PgUp` / `PgDn` |
| Rotate left / right | `Ctrl+L` / `Ctrl+R` |
| Move the page | `Alt+↑` / `Alt+↓` |
| Delete / undo | `Del` / `Ctrl+Z` |
| Zoom | `+` `−` `0` (fit page) `W` (fit width) |
| Page / scan area | `Tab` |
| All shortcuts | `F1` |

## Requirements (Linux)

- SANE (`libsane1`); for recent network scanners: `sane-airscan`.
- To build: Rust (2024 edition), the `wasm32-unknown-unknown` target, `trunk`, `tauri-cli` 2, and Tauri's WebKitGTK 4.1 libraries.

```bash
sudo apt install libsane1 sane-utils sane-airscan libwebkit2gtk-4.1-dev
```

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo install trunk tauri-cli
```

libsane is loaded at run time: without SANE, the application starts and explains how to install it.

## Installation on Linux

The `install.sh` script builds the application and installs it with its icon and menu entry (under `~/.local` by default):

```bash
./install.sh --deps
```

| Option | Effect |
|---|---|
| `--deps` | first installs the system packages (apt, dnf, pacman or zypper) |
| `--system` | installs for every user under `/usr/local` (sudo) |
| `--no-build` | installs the already built binary (`target/release/rustscanner`) |
| `--uninstall` | uninstalls (with `--system` for `/usr/local`) |

It installs the WebAssembly target, `trunk` and `tauri-cli` when needed, then reports what is missing at run time (SANE, `scanner` group for USB scanners, `PATH`).

## Windows

On Windows (10 and 11, x64), RustScanner drives scanners through **WIA** (Windows Image Acquisition), built into every system: any scanner Windows recognizes (USB, or network scanners added in *Settings › Bluetooth & devices › Printers & scanners*) shows up in RustScanner. Printing goes through Windows printers, each page at its actual size.

### Installation

The easiest way is the `RustScanner_0.1.0_x64-setup.exe` installer: it installs the application for the current user, without administrator rights, with Start menu shortcuts, and uninstalls from *Settings › Apps*. To deploy on every machine or for every user, `RustScanner_0.1.0_x64_en-US.msi` (or `fr-FR`) installs machine-wide. If WebView2 is missing (older Windows 10), the installer downloads it.

To build and install from source, the `install.ps1` script is the counterpart of `install.sh` (`install.cmd` runs it from Explorer or `cmd.exe`, with the same options):

```powershell
.\install.ps1 -Deps
```

| Option | Effect |
|---|---|
| `-Deps` | first installs missing tools with winget (Rust, Visual Studio C++ build tools) |
| `-System` | installs for every user (MSI, administrator rights) |
| `-NoBuild` | uses the installer already built (`target\release\bundle`) |
| `-Silent` | installs or uninstalls without any window |
| `-Uninstall` | uninstalls |

If Windows refuses to run the script, go through `install.cmd`, or run `powershell -ExecutionPolicy Bypass -File .\install.ps1`. The script installs the WebAssembly target, `trunk` and `tauri-cli` when needed, then checks the WIA service and that a scanner is present.

### Building the installers

```powershell
cargo tauri build
```

`src-tauri/tauri.windows.conf.json` completes the configuration on Windows: the (bilingual) NSIS installer lands in `target\release\bundle\nsis\`, the MSI packages (one per language) in `target\release\bundle\msi\`. Tauri downloads the NSIS and WiX tools on the first build.

## Build and run

```bash
cargo tauri dev
```

```bash
cargo tauri build --bundles deb
```

The package lands in `target/release/bundle/deb/` (on Windows: see above).

### Working on the interface in a browser

`trunk serve` serves the interface at http://localhost:1420; outside Tauri, `frontend/devshim.js` simulates the core (scanners, scanning, pages) so you can iterate on the UI without a scanner.

### Virtual scanner for testing

sane-backends ships a test scanner. To use it without touching the system configuration:

```bash
mkdir -p /tmp/sane-test && echo test > /tmp/sane-test/dll.conf
```

```bash
SANE_CONFIG_DIR=/tmp/sane-test cargo tauri dev
```

## Tests

```bash
cargo test
```

The `src-tauri/tests/sane_test_backend.rs` integration test drives SANE's virtual scanner (color, 1-bit line art, preview, feeder, simulated jam, cancellation); it is skipped when that backend is not installed.

On Windows, `src-tauri/tests/wia_backend.rs` lists WIA scanners and reads the first one's options; the preview and a line-art scan of an area, which run the scanner, only start on request:

```powershell
cargo test -p rustscanner --test wia_backend -- --include-ignored --nocapture
```

## Architecture

```
shared/      shared types (options, pages, events, settings) and i18n (tr!, trf!)
src-tauri/   Tauri core
  sane.rs      libsane binding (loaded at run time; Linux, macOS)
  wia.rs       WIA 2.0 scanners presented as SANE options (Windows)
  scanner.rs   dedicated scanner thread: commands, scanning, preview, cancellation
  frames.rs    SANE frames (gray, RGB, 3-pass; 1, 8, 16 bits) → image
  document.rs  session pages (PNG on disk, thumbnails in memory)
  export.rs    PDF, images, CUPS printing
  winprint.rs  GDI printing (Windows)
  gettext.rs   option translations (sane-backends .mo catalog)
  commands.rs  commands invoked by the interface
frontend/    Leptos interface (header, sidebar, viewer, pages, dialogs…)
```

Images reach the interface through the `scan://` protocol (JPEG thumbnails and views) rather than over IPC.

## License

MIT
