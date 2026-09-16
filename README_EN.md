<div align="center">
  <img src="src-tauri/icons/icon.png" width="96" alt="QuotaMate icon" />
  <h1>QuotaMate</h1>
  <p><strong>Keep Codex quota in your menu bar, system tray, or desktop pet</strong></p>
  <p><a href="README.md">简体中文</a> · <a href="README_EN.md">English</a></p>
  <p>
    <img alt="Windows 10 / 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows" />
    <img alt="macOS 10.15+" src="https://img.shields.io/badge/macOS-10.15%2B-000000?logo=apple" />
    <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri" />
    <img alt="Local first" src="https://img.shields.io/badge/Data-Local%20first-24B47E" />
  </p>
  <p><a href="https://github.com/yueyisui/QuotaMate/releases/latest"><strong>Download the latest release</strong></a> · <a href="#first-run">First run</a> · <a href="#build-from-source">Build from source</a></p>
</div>

QuotaMate is a lightweight Codex quota companion. It reads the **5-hour quota, weekly quota, reset times, and reset credits** returned by the local Codex CLI, then keeps them visible through a compact display or a desktop pet.

It reuses your existing Codex sign-in. There is no token to paste, no web scraping, and no QuotaMate-hosted service receiving your usage data.

> [!IMPORTANT]
> QuotaMate is an independent community project, not an official OpenAI product. Current releases support Windows x64 and Apple Silicon Macs.

## At a glance

| Capability | What it does |
| --- | --- |
| Live quota | Shows 5-hour and weekly remaining quota, countdowns, and exact reset times |
| Two glanceable modes | Native macOS menu bar / Windows compact widget, or a cross-platform desktop pet |
| Custom pets | Imports transparent PNG, WebP, or animated GIF files and keeps a local history |
| Local first | Talks directly to the local Codex App Server, with no extra quota service |
| Scheduler | Runs one minimal, read-only Codex session at selected local times |
| Desktop-friendly | Bilingual UI, multi-monitor support, position memory, opacity, and autostart |

## Compact mode

Compact mode keeps only the numbers that matter. Show `5h`, `W`, or both. It is mutually exclusive with pet mode, so quota is never duplicated across the desktop.

<p align="center">
  <img src="docs/images/compact-mode-showcase.svg" width="100%" alt="QuotaMate's actual layout in the macOS menu bar and Windows compact widget" />
</p>

### macOS: native menu bar quota

- Appears as the **QuotaMate monochrome logo + `5h 50% · W 63%`** in the right side of the menu bar, without a normal desktop widget.
- Left-click to open a detailed quota panel underneath; it closes when focus moves away or the pointer leaves.
- Right-click for display modes, refresh, settings, and exit.
- Clicking the Dock icon restores the main window after it has been closed or minimized.

### Windows: floating compact widget

- Uses a borderless widget whose width adapts to the selected quota windows.
- Click for details and move the pointer away to collapse; drag it, lock its position, or keep it on top.
- Right-click to open the main window, switch to pet mode, or hide the widget.

## Desktop pet

The pet turns the frequently updated 5-hour quota into a visible energy state. Its expression, color, and energy bar make the current headroom easy to understand at a glance.

<p align="center">
  <img src="docs/images/pet-energy-demo.gif" width="800" alt="Desktop pet animation changing with the 5-hour quota" />
</p>

| 5h remaining | State | Visual feedback |
| --- | --- | --- |
| `75%–100%` | Fully charged | Blue-violet energy, happy expression, active glow |
| `45%–74%` | Doing well | Green energy, relaxed expression, gentle floating |
| `20%–44%` | Running low | Orange cue and a shorter energy bar |
| `0%–19%` | Needs a recharge | Red cue, tired expression, low-energy pose |

Built-in choices include a violet fox, dog, rocket, car, and robot. Click the pet for full details, move away to collapse, drag with the left button, or right-click for shortcuts.

### Make the companion yours

Animals, cartoon avatars, personal logos, pixel art, and animated stickers all work well as custom pets. Here are a few original examples designed to remain clear at desktop-widget sizes:

<table>
  <tr>
    <td align="center"><img src="docs/images/custom-pet-cat.png" width="140" alt="Orange cat custom pet example" /><br /><sub>Pixel-style cat</sub></td>
    <td align="center"><img src="docs/images/custom-pet-capybara.png" width="140" alt="Capybara custom pet example" /><br /><sub>Cartoon capybara</sub></td>
    <td align="center"><img src="docs/images/custom-pet-astronaut.png" width="140" alt="Astronaut custom pet example" /><br /><sub>Tiny astronaut</sub></td>
    <td align="center"><img src="docs/images/custom-pet-logo.png" width="140" alt="Abstract logo custom pet example" /><br /><sub>Personal logo</sub></td>
  </tr>
</table>

- Supports transparent `PNG`, `WebP`, and animated `GIF`; square artwork with a clean silhouette works best.
- Imported files are copied into QuotaMate's local configuration directory, so the source file can be moved later.
- Import history makes it easy to switch pets or delete local copies.
- Static artwork keeps its original appearance, animated GIFs keep moving, and the outer energy glow still follows quota.
- Pet size, overall opacity, and always-on-top behavior are configurable.

> [!NOTE]
> Only import artwork you are allowed to use. The four examples above are original demo assets created for this project and do not use third-party brands or characters.

## Dashboard and scheduler

The main window brings together the account plan, both quota windows, reset times, reset credits, the next scheduled run, and the last update time. Fields not returned by the service are shown as unavailable instead of being guessed.

The scheduler can start one minimal Codex session at multiple local times:

- Runs in a dedicated QuotaMate directory with a read-only sandbox, never in your project directory.
- Does not save conversations or retry automatically; each run is limited to 120 seconds and each schedule runs at most once per day.
- Does not run while the computer is shut down or asleep, or while QuotaMate is fully exited. Missed runs are not replayed.

> [!WARNING]
> Scheduled runs call Codex and may consume a small amount of quota. Leave the schedule empty or disable it if you do not need this feature.

## Download and install

Open [GitHub Releases](https://github.com/yueyisui/QuotaMate/releases/latest) and download the file for your system.

| Platform | Current support | Recommended file | Installation |
| --- | --- | --- | --- |
| Windows | Windows 10/11 x64 | `QuotaMate_<version>_x64-setup.exe` | Run the installer, or use the standalone `quotamate.exe` |
| macOS | macOS 10.15+, Apple Silicon | `QuotaMate_<version>_aarch64.dmg` | Open the DMG and drag QuotaMate into Applications |

Before launching, make sure the Codex CLI—or a Codex desktop app that includes it—is installed and signed in. Windows also requires the WebView2 Runtime (already present on most Windows 10/11 systems); macOS uses the system WebKit.

> [!WARNING]
> Current packages are not signed with a trusted code-signing certificate. Download only from this repository. Windows may show a SmartScreen prompt. On macOS, first launch may require right-clicking the app in Finder and choosing Open, or approving it under System Settings → Privacy & Security.

### First run

1. Confirm that `codex --version` works in a terminal and that Codex is signed in.
2. Launch QuotaMate and wait for the first quota snapshot.
3. Open Settings and choose compact/menu bar mode, desktop pet, or hidden.
4. Adjust the displayed windows, refresh interval, opacity, pet size, and autostart as needed.
5. Closing the main window leaves QuotaMate resident. Use Exit in the menu bar or system tray to quit completely.

## Interaction reference

| Platform and location | Action | Result |
| --- | --- | --- |
| macOS menu bar | Left-click | Toggle the detailed quota panel |
| macOS menu bar | Right-click | Open the shortcut menu |
| macOS Dock | Click the app icon | Restore a closed or minimized main window |
| Windows system tray | Left-click | Open the main window |
| Windows system tray | Right-click | Open the shortcut menu |
| Windows compact widget | Click / drag / right-click | Details / move / shortcut menu |
| Desktop pet | Click / drag / right-click | Details / move / shortcut menu |
| Expanded details | Move the pointer away | Return to compact or pet form |

Hidden mode removes live quota text and the desktop pet while keeping the menu bar/system tray entry available for reopening the app or changing modes.

## Local data and privacy

<p align="center">
  <img src="docs/images/local-data-flow.svg" width="900" alt="QuotaMate local data flow" />
</p>

QuotaMate locates `codex` on the machine, starts the official Codex App Server, and sends quota data to the UI over local Tauri IPC.

- Does not ask you to enter, copy, or import an access token.
- Does not write Authorization headers, cookies, or Codex conversation content.
- Does not upload quota data to a QuotaMate-hosted server or another third-party service.
- Keeps settings, custom pet images, and runtime logs locally; likely authentication data is redacted from logs.
- Uses temporary sessions, a dedicated working directory, and a read-only sandbox for scheduled runs.

## FAQ

<details>
<summary><strong>Why does QuotaMate say Codex is unavailable or keep waiting for data?</strong></summary>

Run `codex --version` in a terminal first and confirm the CLI is installed and signed in. Then choose Refresh usage from the menu bar/system tray, or restart QuotaMate.
</details>

<details>
<summary><strong>Why is QuotaMate still running after I close the main window?</strong></summary>

It stays resident to update the menu bar, system tray, or desktop pet. Choose Exit from the shortcut menu to quit completely.
</details>

<details>
<summary><strong>Why is a quota window, account plan, or reset credit unavailable?</strong></summary>

Different Codex CLI versions, sign-in methods, and account types can return different fields. QuotaMate only displays data actually returned by the service.
</details>

<details>
<summary><strong>Will schedules run while the computer is off?</strong></summary>

No. Runs are skipped while the computer is shut down or asleep, or while QuotaMate is fully exited, and they are not replayed later.
</details>

## Build from source

Common prerequisites: Node.js, pnpm, Rust stable, and an installed, signed-in Codex CLI.

```bash
pnpm install
pnpm tauri dev
```

### macOS

An Apple Silicon Mac and Xcode Command Line Tools are required:

```bash
pnpm tauri:build:mac
```

Artifacts are written to `artifacts/macos-arm64/`. The script stages the build in a local temporary directory to avoid AppleDouble files created by some external drives.

### Windows

Visual Studio C++ Build Tools (MSVC) are required:

```powershell
pnpm tauri:build:windows
```

Artifacts are written to `artifacts/windows-x64/`, including the NSIS installer and standalone EXE.

### Checks

```bash
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

## Stack and layout

- [Tauri 2](https://tauri.app/) for cross-platform windows, menu bar/system tray, and local IPC
- [Rust](https://www.rust-lang.org/) for Codex App Server integration, configuration, scheduling, and window management
- [React](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/) for the dashboard, widgets, and pets
- [Vite](https://vite.dev/) for frontend development and builds

```text
src/                         React UI, pets, and localization
src-tauri/src/codex/         CLI discovery, App Server, and quota parsing
src-tauri/src/config/        Local configuration and migrations
src-tauri/src/scheduler/     Daily scheduled runs
src-tauri/src/tray/          macOS menu bar and Windows system tray
src-tauri/src/windows/       Main and floating window lifecycle
docs/images/                 README visuals and demo assets
```

## Feedback

QuotaMate is still an early-stage project. Bug reports, feature requests, and UI feedback are welcome in [GitHub Issues](https://github.com/yueyisui/QuotaMate/issues). Remove account credentials and other sensitive information before sharing logs or screenshots.
