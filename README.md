# Tapkin

A tiny desktop typing companion for **macOS 11+ and Windows 11**, built with Rust and Tauri 2.

## Run and build

Install Rust stable, Node.js **22.12+** and **pnpm 12.9+**, plus the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/):

- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Windows: Visual Studio Build Tools with “Desktop development with C++” and Microsoft Edge WebView2 Runtime. WebView2 is already included with Windows 11; the installer can bootstrap it when missing, which requires an internet connection for that installation step.

```sh
pnpm ci
pnpm run tauri dev
```

```sh
pnpm run tauri build
```

Build on the destination OS. macOS outputs an `.app` and `.dmg` under `src-tauri/target/release/bundle`; Windows outputs an `.exe`, NSIS installer and MSI where the toolchain permits. To build only the supported installer used by CI:

```sh
# macOS
pnpm run tauri build --bundles app,dmg
# Windows
pnpm run tauri build --bundles nsis
```

## WebView styling compatibility

The frontend build explicitly targets Safari 14-era WKWebView and Chromium/Edge
87 or newer instead of relying on Vite's default browser baseline. Settings uses
margin-based flex spacing (including wrapped permission buttons), a `:focus`
fallback for keyboard focus, and text-wrapping and slider-track fallbacks. Engines
without `accent-color` retain usable native checkboxes rather than requiring
custom checkbox scripting. No runtime CSS polyfill or extra dependency is needed.

This is a compatibility baseline for the app's supported macOS 11+ and Windows 11
platforms, not a guarantee for every historical WebView. Native window behavior,
system fonts, and controls still need testing on the destination OS.

## Use

- Drag the pet with the left mouse button. Right-click it to open Settings.
- Open Settings from the menu-bar icon (macOS) or notification-area icon (Windows). Windows may place the icon in the notification-area overflow.
- Choose a skin folder, reload edited files, or open the current folder from Settings/the tray.
- Resize using the Settings slider: the pet resizes live while you move it, with previews limited to about 30 requests per second. Releasing the slider or finishing a keyboard adjustment saves the final size. Image and window aspect ratios stay matched; both dimensions are capped at 800 logical pixels.
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

A skin is a local directory. The bundled `skins/example` contains a typing tabby pixel cat with a square head, on three transparent 256 × 256 PNG canvases; the app icons use the same character. On launch, missing example files are installed into the application's writable data directory; existing files are preserved. Updating the app therefore keeps an existing example copy, including its previous artwork. To use the new artwork in that case, choose a separate folder containing the current `skins/example` files. Choose another directory to use a custom skin.

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
```

- `idle` and at least two `typing` frames are required. Two to sixteen typing frames are supported; unknown config fields and unreferenced reaction files are ignored.
- Images must be valid static PNGs, with the same canvas size (1–4096 pixels per axis). Transparency is supported. There is no automatic cropping, scaling alignment or layered animation.
- File references must stay inside the skin folder, including after symlink resolution. Absolute paths and `..` components are rejected. Config files are limited to 64 KiB, individual PNG files to 16 MiB, all referenced PNG files to 32 MiB in total, and their combined canvases to 16 megapixels.
- Animation timing is controlled in **Settings → Animation** and saved in the client's `settings.json`, independently of skins. Typing timeout defaults to 180 ms (50–10000 allowed); frame hold defaults to 60 ms (0 through the timeout allowed). Valid edits apply and autosave together after a 400 ms pause, or immediately when a field change is committed. Invalid/incomplete edits are not saved. Changes apply across all skins, including mapped images. Legacy `typing_timeout_ms` and `frame_hold_ms` entries in `pet.toml` are ignored.
- **Treat held keys as repeated presses** is enabled by default in Settings → Animation. Disable it to advance/select an image only on the initial press, ignore auto-repeat, and keep the latest selected image while any observed key is held. A different key can still select another image, respecting frame hold. The idle timeout starts after the last held key is released. This app-wide preference persists across restarts; it does not change keyboard repeat in other apps. Held-key state is transient and cleared on skin replacement, listener restart/failure, or input-queue overflow.
- Each key advances the logical typing index and extends the idle deadline. Frame hold limits visual swaps during rapid bursts, displaying the latest requested frame when the hold expires. Set `frame_hold_ms = 0` to display every press immediately. There is no automatic animation while inactive.
- An invalid reload shows an error and keeps the currently loaded skin. On startup, an unavailable/invalid selected skin falls back to the example with a visible warning.

### Custom key images

Add an optional `[key_mappings]` table **after the other settings** in your skin's
`pet.toml`, then choose **Reload skin**:

```toml
[key_mappings]
KeyA = "typing_1.png"
KeyB = "typing_1.png" # Multiple keys can share one image.
Space = "typing_2.png"
Enter = "idle.png"
"?" = "typing_1.png" # Match the character, including Shift/layout translation.
"!" = "typing_2.png"
```

You can also reference new PNGs inside the skin folder, provided they match the
idle canvas size. Mapped images follow the existing path, PNG, file-size and
total-memory checks. Shared mapped files are cached once and can reuse existing
idle/typing frames. Invalid mappings leave the current skin intact.

A mapped key requests its image until `typing_timeout_ms` elapses without another
press. Repeated presses extend that deadline; another key requests its own image,
subject to `frame_hold_ms`. Unmapped keys continue the normal typing cycle. All
presses advance that cycle, even when a mapped image is displayed. Existing skins
without the table behave as before. This selects images; it does not remap,
suppress, or synthesize keyboard input.

Quoted single-character names such as `"?"`, `"!"`, `"a"`, and `"A"` match the
character associated with a key press. Character matching is case-sensitive and
takes priority over the physical-key mapping; otherwise the physical mapping or
normal typing animation is used. Character keys must contain exactly one
printable Unicode scalar (not a word, control character, or combining sequence).

Physical-key names are case-sensitive **physical key positions**: `KeyA`
means the A position on a US keyboard even with a different layout or Shift held.
Supported names:

- `KeyA`–`KeyZ`, `Digit0`–`Digit9`, and `F1`–`F12`.
- `Space`, `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`.
- `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Home`, `End`, `PageUp`, `PageDown`.
- `Minus`, `Equal`, `BracketLeft`, `BracketRight`, `Backslash`, `Semicolon`,
  `Quote`, `Backquote`, `Comma`, `Period`, `Slash`.
- `Numpad0`–`Numpad9`, `NumpadDecimal`, `NumpadAdd`, `NumpadSubtract`,
  `NumpadMultiply`, `NumpadDivide`, `NumpadEnter`.

Modifier-only keys, shortcut combinations, media/vendor-specific keys, and a
Settings mapping editor are not supported. Function-key mappings require the OS
to deliver a function-key event rather than a media action. OS permission and
secure-input restrictions still apply.

Character matching is best-effort keyboard-event translation, not observation of
text committed to another application's input field. IME composition, dead-key
sequences, paste, and multi-character commits are not supported. macOS uses the
Unicode value on the event; Windows translates using the foreground keyboard
layout without changing the OS dead-key state. Control/Command/Windows shortcuts
do not produce character mappings; Windows permits Ctrl+Alt for AltGr layouts.

## Local settings and privacy

Settings are human-readable `settings.json` in Tauri's application configuration directory. Skin copies are under `skins/` in its application data directory (usually `~/Library/Application Support/io.tapkin.desktop` on macOS and `%APPDATA%\io.tapkin.desktop` on Windows). Saves use a flushed temporary file and atomic replacement. Corrupt settings restore defaults with a warning. Quit flushes the current position, and dragging uses a debounced save.

Rust owns the keyboard hook, animation, timing, skin validation and window settings. Native hooks inspect physical key identity and translate a bounded single character transiently for image selection. They do not accumulate typed text. Only skin images, frame indexes and app status reach the frontend, not a stream of key identities or characters. No key/text history is kept and no observed keyboard data is logged, persisted or sent over a network. Only user-configured mapping rules are stored in `pet.toml`. The input queue is bounded and OS callbacks never wait on ordinary input.

The native backend abstraction uses a listen-only CoreGraphics `CGEventTap` on macOS and `WH_KEYBOARD_LL` on Windows. These acknowledge startup success/failure, handle tap disablement and stop/retry listeners. Both observe key-down and key-up events for hold tracking; macOS modifier-only flag changes are not subscribed, while Windows key-down includes modifier keys. Neither hook suppresses or modifies the user's input.

## Checks

### Frontend layout

The web frontend lives under `src/`. `main.ts` is the minimal entry point;
`shared/` contains application bootstrap/events, IPC types, DOM helpers and
autosave; `settings/` contains Settings rendering, timing and resize controls;
Pet interactions live alongside application setup in `shared/application.ts`.
CSS has three files under `styles/`: `base.css` for shared dimensions and the
pet window, `controls.css` for form controls, and `settings.css` for the complete
Settings layout. `main.ts` imports them in that order.
`index.html` remains the shared window markup so native window URLs are unchanged.

Reused dimensions live in `styles/base.css`. Layout spacing follows a 4 px
scale; shared typography, radii, borders and control sizes use named tokens.
Values used once or twice, and geometry repeated only for browser fallbacks,
stay hard-coded in their component CSS. Development-only
controls and their section spacing live with the other Settings sections in
`styles/settings.css`. Timing and resize logic are grouped with rendering in
`settings/index.ts`.

Run frontend tests with `node --test tests/*.test.mjs`.

```sh
pnpm run check
pnpm run build
cd src-tauri
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The tests exercise PNG/config validation and path confinement, deterministic animation/idle timing, settings recovery/atomic replacement, multiple-monitor position restoration and the injected core with a non-Tauri overlay renderer. The renderer contract suite runs with `cargo test --locked --test overlay_contract`. In `tauri dev`, Settings exposes a local test button: focus it and press keys while the global listener is inactive. It sends only a generic typing signal. This UI is hidden and the command is disabled in production.

Linux can run Rust core unit tests and the frontend build; **Linux/Wayland desktop support is intentionally absent**. The binary reports an unsupported target there. See [VALIDATION.md](VALIDATION.md) for actual verification and the native manual acceptance checklist. An OS cross-check alone does not prove transparent windows, permissions or global hooks work on a physical desktop.
