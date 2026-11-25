# Lumina (v0.3.0)

Native **Rust** (`windows 0.62` + `Windows.UI.Composition` + Direct2D/Direct3D11) desktop companion implementing a Liquid Glass Dynamic Island / MacBook Notch, First-run onboarding tour, interactive HSV color-wheel desktop customization, draggable Anurati Day Clock, live cursor-following Desktop Widgets, optional bottom Glass Taskbar, and 14-section searchable Settings window.

## Workspace Architecture

| Crate | Binary / Role | Responsibilities |
|---|---|---|
| `crates/core` | `np-core` | Settings & 10 surface themes (`tint` / `tint_bottom` / `glow`), `IslandEdge::{Top, Left, Right}`, `OpenTarget::{LastTab, Music, Widgets}`, `AiProvider`, HSV/RGB/Hex helpers, Dynamic Island state machine & 8 tabs (`Music`, `Widgets`, `Panels`, `Clipboard`, `Tools`, `Ai`, `Quick`, `Calls`), Focus Timer (`elapsed_frac`), calendar, Anurati Day Clock vector glyphs, **SDF anti-aliased symbols**, dock magnification layout, QR encoder, crash marker |
| `crates/platform` | `np-platform` | WinRT SMTC media + loopback audio spectrum, Core Audio volume/mic mute, DDC/WMI brightness & dimmer, Wi-Fi/Bluetooth/Focus toggles, CPU/GPU/RAM/Net telemetry, clipboard history, screenshot, `ms-screenclip:` Snip trigger, pure-Rust MJPEG `.avi` screen recorder (`Recorder`), `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE` Stream Privacy Shield), eyedropper color picker, LAN QR file server, Windows Run startup (`startup.rs`), `IFileOpenDialog` picker, taskbar hide/restore + `foreground_is_maximized`, hotkeys |
| `crates/ui` | `np-ui` | Direct3D11/Direct2D surface synthesis (`Gfx`), `DispatcherQueueController` + `DesktopWindowTarget` (`Host`), 4-tier Liquid Glass backdrop (`GlassPanel`), spring animations (`motion`), visual painter (`Painter`: `card`, `grad_rect`, `stroke_rect`, `soft_shadow`, `ring_progress`, `sparkline`, `layered_weather`, `text_shadow`, `pill_text`, glowing `wavy_progress`) |
| `crates/guard` | `lumina-guard.exe` | Sidecar watchdog monitoring the main process handle and restoring `Shell_TrayWnd` on abnormal exit |
| `crates/app` | `lumina.exe` | Tray host + ordered shutdown, First-run onboarding tour (`tour.rs`), 8-tab multi-edge Dynamic Island / Notch (`island.rs`) with live widget drag-out & Glare Strip & HUD flashes, soft-shadowed Desktop Day Clock (`day_clock.rs`), interactive Desktop Widgets (`desktop_widgets.rs`), auto-hiding Glass Taskbar dock (`taskbar_bar.rs`), and 14-section searchable Settings window (`settings_win.rs`) |

## Key Features (`v0.3.0`)

- **First-run Onboarding Tour (`crates/app/src/tour.rs`)**: Opens automatically when `settings.json` is absent (or via `--open-tour` / Settings → About → Replay), letting users pick `Same as Windows` / `English` / `Tiếng Việt`, preview the Island (`Try it now`), and toggle `Optional extras` (`Notifications in the island`, `Game strip`, `FPS counter`, `Volume in the island`, `Clipboard history`).
- **Multi-Edge 8-Tab Dynamic Island (`crates/app/src/island.rs`)**:
  - **Edges & Resting Modes**: Dock to `Top`, `Left`, or `Right` monitor edges (`IslandEdge`), shrink to a `5dp` **Glare strip** under maximized windows, flash transient Volume/Brightness/Notification HUD pills, respect `Open to` (`Last tab` / `Music` / `Widgets`), `Keep open until clicked outside`, and `Longer hover dwell`.
  - **8 Interactive Tabs**:
    1. **Music**: Album-art glow, SMTC controls, wavy progress scrubber, and `Bars`/`Wave`/`Dots` visualizer
    2. **Widgets**: 2x2 Bento (`Calendar`, `Focus Timer`, `Weather`, `System Monitor`) — grab and drag any tile live onto the desktop to pin it (`desktop_widgets::preview_at` + `spawn_at`)
    3. **Panels**: Persistent Quick Notes synced to `settings.json`
    4. **Clipboard**: Live clipboard history with index indicators, pinning, and click-to-copy
    5. **Tools**: `Screenshot`, `Snip` (`ms-screenclip:`), pure-Rust MJPEG `.avi` `Record` (`Recorder` with live `REC MM:SS` pill), `Pick color` eyedropper, `Send QR` LAN transfer, File Shelf, and 5 customizable `+ Add` app/file shortcut slots
    6. **AI agents**: One-click launchers for `Antigravity`, `ChatGPT`, `Claude`, `Gemini`, and `Perplexity`
    7. **Quick**: Brightness/Volume sliders, Wi-Fi/Bluetooth/Focus/Dark toggles, Power Mode
    8. **Calls**: Universal Microphone Mute (`Ctrl+Alt+M`), live Mic/Cam activity indicators, and `WDA_EXCLUDEFROMCAPTURE` Stream Privacy Shield
- **14-Section Searchable Settings (`crates/app/src/settings_win.rs`)**: Real-time search filter, smooth mouse-wheel scrolling, and all 14 video sections (`General`, `Accessibility`, `Appearance` with `See-through surface` + Live Aurora Preview + 10 Themes, `Island`, `Taskbar` with Windows taskbar shortcut & side auto-hide, `Panels`, `Widgets` with interactive **HSV Color Wheel + Value Bar + `#RRGGBB` Hex Readout**, `Calls`, `Streaming`, `Gaming`, `AI agents`, `Tools` 5-slot picker, `Keys`, `About`).
- **Global Hotkeys (`crates/platform/src/hotkeys.rs`)**:
  - `Ctrl+Alt+N`: Toggle Dynamic Island
  - `Ctrl+Alt+M`: Toggle Universal Microphone Mute
  - `Ctrl+Alt+V`: Open Clipboard History in Island

## Build & Run

Requires Rust (`stable-x86_64-pc-windows-msvc`), Visual Studio MSVC Build Tools, and Windows 10/11 SDK.

```powershell
# Run unit tests across all crates
cargo test --workspace --locked

# Build release binaries (build to a clean target dir if an earlier lumina.exe is running)
cargo build --release --workspace --locked

# Safe isolated launch (keeps existing Windows taskbar intact)
$env:LUMINA_CONFIG_DIR = "$PWD\target\cfg-test-v07"
.\target\release\lumina.exe --keep-taskbar --open-settings
```

## CLI Flags

- `--keep-taskbar`: Keep the native Windows taskbar visible while running
- `--open-settings`: Show the Settings window immediately on startup
- `--open-tour`: Open the First-run onboarding tour window on startup
- `--verify-windows`: Present Settings (and tour if `--open-tour` is passed) + desktop widgets, then exit cleanly after `1200ms`
- `--demo-media`: Seed demo media metadata (`Static Bloom` — `Juno Vale`) when no SMTC session is active
- `--exit-after <secs>`: Exit cleanly after the given number of seconds
- `LUMINA_CONFIG_DIR`: Override config, state, and `lumina.log` directory (defaults to `%LOCALAPPDATA%\LuminaRS`)

## Build the Windows x64 EXE installer

Run the release build from an x64 Visual Studio Developer PowerShell with the Windows SDK resource compiler (rc.exe) on PATH. Alternatively set the RC environment variable to the full path to rc.exe. Install Inno Setup 7 and run the following from the repository root.

```powershell
cargo build --workspace --release --locked
ISCC.exe packaging/windows_setup.iss
```

The installer is written to target/Lumina-Setup-x64.exe. It installs for the current user and includes the app, guard, fonts, Start menu shortcut and uninstaller. The optional /NOREGISTRY flag skips registration in Windows Installed Apps. In that mode, run unins000.exe from the installation directory to uninstall.

The current app targets Windows x64.
