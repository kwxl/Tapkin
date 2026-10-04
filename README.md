# Tapkin v1

A tiny desktop typing companion for **macOS 11+ and Windows 11**, built with Rust and Tauri 2. A transparent, frameless pet sits above your apps, switches PNG frames when you type, and rests when typing stops. No accounts, telemetry, text capture, or runtime network requirement.

## Architecture

Tauri owns Settings, tray/menu and application lifecycle. The floating pet is injected into `TapkinApp` as `Arc<dyn OverlayRenderer>`; its current `TauriOverlayRenderer` preserves the WebView image-swap mechanism. Core typing, skin and settings logic uses project-owned types and can run with a non-Tauri test renderer. See [ARCHITECTURE.md](ARCHITECTURE.md) for the contract, boundaries and future native-overlay integration.

## Run and build

Install Rust stable and Node.js **22.12+**, plus the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/):

- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Windows: Visual Studio Build Tools with “Desktop development with C++” and Microsoft Edge WebView2 Runtime. WebView2 is already included with Windows 11; the installer can bootstrap it when missing, which requires an internet connection for that installation step.

```sh
npm ci
npm run tauri dev
```

```sh
npm run tauri build
```

Build on the destination OS. macOS outputs an `.app` and `.dmg` under `src-tauri/target/release/bundle`; Windows outputs an `.exe`, NSIS installer and MSI where the toolchain permits. To build only the supported installer used by CI:

```sh
# macOS
npm run tauri build -- --bundles app,dmg
# Windows
npm run tauri build -- --bundles nsis
```

The GitHub Actions workflow in `.github/workflows/desktop.yml` runs formatting, Clippy, tests, the frontend build and installer builds on `macos-latest` and `windows-latest`. The workflow retains unsigned installers as artifacts and does not publish releases. Native builds use the runner's CPU architecture; use an Intel Mac or Apple Silicon Mac to build for that architecture.

Development artifacts are unsigned and are not Apple notarized. macOS Gatekeeper or Windows SmartScreen may warn; verify the source/build and use the OS's normal trusted-app opening flow. Do not disable security protections. Signing/notarization can be added to distribution CI later.

## Use

- Drag the pet with the left mouse button. Right-click it to open Settings.
- Open Settings from the menu-bar icon (macOS) or notification-area icon (Windows). Windows may place the icon in the notification-area overflow.
- Choose a skin folder, reload edited files, or open the current folder from Settings/the tray.
- Resize using the Settings slider; image and window aspect ratios stay matched.
- Always on Top, Click Through, Lock Position and Launch at Login are available in Settings and the tray.
- **Click Through can always be turned off from the tray**, even when the pet no longer receives pointer input. Closing Settings hides it; Quit in the tray ends the app.
- Window size and physical position survive relaunch. If a saved monitor is absent or the pet would be outside its display, it moves onto the primary display. macOS places it across Spaces where the window system permits.
- Launch at Login defaults to off. Enable it after installing/moving the app to its final location; changing the executable location may require toggling it again. The optional integration uses a macOS LaunchAgent or Windows per-user startup entry.

## macOS keyboard permissions

Tapkin needs permission to detect when a key is pressed while another application is focused. **It does not record or save what you type.**

If the OS denies access, Settings opens with “Keyboard listener inactive” and an actionable explanation:

1. Open **System Settings → Privacy & Security → Input Monitoring** from Settings.
2. Enable Tapkin. If the event tap still cannot be created, check **Accessibility** using the second shortcut.
3. Select **Retry listener**. If macOS explicitly asks to relaunch, quit and reopen Tapkin; a machine reboot is not required by the app.

During development, macOS may associate permission with the development executable or terminal instead of the final `.app`. Retest permissions on the installed bundle. Password fields, Secure Keyboard Entry, exclusive full-screen apps and OS security boundaries can prevent keyboard observation. Tapkin does not bypass them.

Windows hook startup errors also appear in Settings with a Retry listener button. Typing in elevated applications or secure desktops may not be observable from the normal per-user process.

## Skins

A skin is a local directory. The bundled `skins/example` contains original pixel-cat artwork. On first launch, its files are installed into the application's writable data directory; existing files are preserved. Choose another directory to use a custom skin.

```text
my-cat/
  pet.toml
  idle.png
  typing_1.png
  typing_2.png
```

```toml
name = "My Cat"
idle = "idle.png"
typing = ["typing_1.png", "typing_2.png"]
typing_timeout_ms = 180
frame_hold_ms = 60
```

- `idle` and at least two `typing` frames are required. Two to sixteen typing frames are supported; unknown config fields and unreferenced reaction files are ignored.
- Images must be valid static PNGs, with the same canvas size (1–4096 pixels per axis). Transparency is supported. There is no automatic cropping, scaling alignment or layered animation.
- File references must stay inside the skin folder, including after symlink resolution. Absolute paths and `..` components are rejected. Config files are limited to 64 KiB, individual PNG files to 16 MiB, all referenced PNG files to 32 MiB in total, and their combined canvases to 16 megapixels.
- `typing_timeout_ms` defaults to 180 and accepts 50–10000. `frame_hold_ms` defaults to 60 and accepts 0 through the timeout value.
- Each key advances the logical typing index and extends the idle deadline. Frame hold limits visual swaps during rapid bursts, displaying the latest requested frame when the hold expires. Set `frame_hold_ms = 0` to display every press immediately. There is no automatic animation while inactive.
- An invalid reload shows an error and keeps the currently loaded skin. On startup, an unavailable/invalid selected skin falls back to the example with a visible warning.

## Local settings and privacy

Settings are human-readable `settings.json` in Tauri's application configuration directory. Skin copies are under `skins/` in its application data directory (usually `~/Library/Application Support/io.tapkin.desktop` on macOS and `%APPDATA%\io.tapkin.desktop` on Windows). Saves use a flushed temporary file and atomic replacement. Corrupt settings restore defaults with a warning. Quit flushes the current position, and dragging uses a debounced save.

Rust owns the keyboard hook, animation, timing, skin validation and window settings. Native hooks normalize input immediately into `AnyKeyPressed`: neither hook reads printable characters or a key code. Only skin images, frame indexes and app status reach the frontend. No key history is kept and no keyboard data is logged, persisted or sent over a network. The input queue is bounded and OS callbacks never wait on ordinary input.

The native backend abstraction uses a listen-only CoreGraphics `CGEventTap` on macOS and `WH_KEYBOARD_LL` on Windows. These were selected over `rdev` to explicitly acknowledge startup success/failure, handle tap disablement and stop/retry listeners. macOS modifier-only changes do not animate because the tap subscribes only to key-down events; Windows key-down includes modifier keys.

## Checks

```sh
npm run check
npm run build
cd src-tauri
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The tests exercise PNG/config validation and path confinement, deterministic animation/idle timing, settings recovery/atomic replacement and multiple-monitor position restoration. In `tauri dev`, Settings exposes a local test button: focus it and press keys while the global listener is inactive. It sends only a generic typing signal. This UI is hidden and the command is disabled in production.

Linux can run Rust core unit tests and the frontend build; **Linux/Wayland desktop support is intentionally absent**. The binary reports an unsupported target there. See [VALIDATION.md](VALIDATION.md) for actual verification and the native manual acceptance checklist. An OS cross-check alone does not prove transparent windows, permissions or global hooks work on a physical desktop.

License: MIT.
