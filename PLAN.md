# PLAN.md — Tapkin v1

## 0. Goal

Build **Tapkin**, a lightweight cross-platform desktop character overlay in **Rust + Tauri 2**.

Tapkin displays a transparent, always-on-top character image. While the user types in any application, the pet switches between typing frames. When typing stops, it returns to an idle frame.

**v1 targets:**
- macOS
- Windows

**Not in v1:**
- Linux / Wayland
- Live2D / skeletal animation
- audio
- networking / cloud sync
- telemetry
- recording or storing typed text
- per-key full keyboard visualization
- plugin marketplace
- complex animation editor

The first release should be small, reliable, easy to skin, and safe to leave running all day.

---

## 1. Product behavior

### 1.1 Core interaction

Default behavior:

1. App launches.
2. A frameless transparent pet window appears.
3. The pet shows `idle.png`.
4. Any global key press switches the pet to a typing frame.
5. Consecutive key presses alternate:
   - `typing_1.png`
   - `typing_2.png`
6. If no key is pressed for `typing_timeout_ms`, return to `idle.png`.
7. The pet can be dragged anywhere on screen.
8. The user can resize it while preserving aspect ratio.
9. The window stays above normal application windows.
10. The app remains usable while another app has keyboard focus.

The app must **never store typed text**.

---

## 2. v1 feature scope

### Required

- Tauri 2 desktop app
- Rust backend
- macOS support
- Windows support
- global keyboard event detection
- transparent frameless pet window
- always-on-top mode
- draggable window
- configurable window size
- typing animation using whole-frame PNG replacement
- idle timeout
- skin folders
- TOML skin configuration
- settings persisted locally
- system tray / menu
- quit action
- open skin folder action
- reload current skin action
- click-through toggle
- launch-at-login optional toggle if reasonably straightforward
- no network requirement
- no telemetry
- release builds for macOS and Windows

### Nice-to-have if low risk

- configurable typing frame timing
- multiple-monitor position restoration
- “lock position” toggle

Do not delay v1 for nice-to-have items.

### Deferred to v1.1

- specific-key mappings
- Enter / Backspace / Escape / Space reactions
- idle expression cycling
- reaction priority / timeout rules

v1 should treat keyboard activity as a generic typing signal rather than exposing key-specific behavior.

---

## 3. Technology choices

### Application shell

Use:

- **Tauri 2**
- Rust stable
- minimal HTML/CSS/TypeScript frontend
- no React/Vue/Svelte unless there is a concrete need

The UI is intentionally tiny: one `<img>` plus a small settings surface.

### Global keyboard input

Create a platform abstraction.

Initial implementation may use:

- `rdev` for macOS and Windows

But do **not** couple the rest of the app directly to `rdev`.

Use an internal trait/interface so platform-specific implementations can replace it later.

Concept:

```rust
pub trait InputBackend: Send + Sync {
    fn start(&self, tx: Sender<InputEvent>) -> Result<()>;
}
```

Common event type for v1:

```rust
pub enum InputEvent {
    AnyKeyPressed,
}
```

Only emit the minimum semantic event needed by the animation engine. Key-specific events belong to v1.1.

Do not expose printable characters to the frontend.

### macOS fallback

If `rdev` proves unreliable on current macOS, implement the macOS backend with CoreGraphics `CGEventTap`.

Possible crates:

- `objc2`
- CoreGraphics/CoreFoundation bindings appropriate for the current Rust ecosystem

Keep this behind the same `InputBackend` abstraction.

### Windows fallback

If `rdev` is unreliable on Windows, implement:

- `WH_KEYBOARD_LL`

or another appropriate native global keyboard hook.

Again, keep it behind `InputBackend`.

---

## 4. Privacy and security requirements

This app observes global keyboard activity, so this is non-negotiable.

### Rules

- Never persist raw key events.
- Never write typed characters to logs.
- Never send keyboard data over the network.
- Do not add analytics.
- Do not add crash reporting that contains input events.
- Do not expose the actual pressed printable key to the frontend unless a future feature explicitly requires it.
- Debug logging must log only state transitions such as:
  - `typing_started`
  - `typing_stopped`
  - `special_action_enter`
- Release builds should default to minimal logging.

### macOS permission UX

Global key monitoring may require Input Monitoring / Accessibility permission.

The app should:

1. detect failure to start the global listener,
2. show a concise explanation,
3. direct the user to the correct macOS privacy settings,
4. allow retry without restarting the whole machine.

Do not pretend the listener is active if permission was denied.

---

## 5. Asset / skin format

A skin is a folder.

Example:

```text
skins/
└── teto/
    ├── pet.toml
    ├── idle.png
    ├── typing_1.png
    ├── typing_2.png
    ├── happy.png
    ├── confused.png
    └── surprised.png
```

Only the first three images are mandatory for v1.

### `pet.toml`

Example:

```toml
name = "Teto"

idle = "idle.png"
typing = [
  "typing_1.png",
  "typing_2.png"
]

typing_timeout_ms = 180
frame_hold_ms = 60
```

### Parsing behavior

- Paths are relative to the skin directory.
- Extra images are allowed and ignored by v1 unless referenced by the schema.
- Missing required files produce a clear validation error.
- Invalid config must not crash the app.
- A skin reload should not require application restart.

### Image assumptions

v1 assumes:

- PNG
- transparent background supported
- all frames are intended to occupy the same logical canvas
- whole-frame replacement, not layered body parts

Do not implement automatic cropping or pose alignment in v1.

---

## 6. State machine

Implement animation logic in Rust as a small deterministic state machine.

Suggested states for v1:

```rust
enum PetState {
    Idle,
    Typing,
}
```

Typing animation tracks a frame index separately. Reaction states are deferred to v1.1.

### Key press behavior

On `AnyKeyPressed`:

- cancel pending idle transition
- if already typing:
  - advance typing frame index
- otherwise:
  - enter `Typing`
  - show first typing frame
- schedule return to idle after `typing_timeout_ms`

### Repeated typing

Every qualifying key press should advance:

```text
typing_1
typing_2
typing_1
typing_2
...
```

Do not animate continuously at a fixed FPS when the user is not pressing keys.

The visual response should be tied to actual input.

---

## 7. Window behavior

The pet window must be:

- transparent
- frameless
- always-on-top
- hidden from normal app chrome where possible
- draggable
- resizeable through app controls or modifier + wheel
- aspect-ratio preserving
- position persistent between launches

### Click-through

Provide a toggle.

When enabled:

- pointer events pass through the pet
- keyboard listener remains active

There must be a reliable way to disable click-through again from:

- tray/menu item

Do not create a state where the user can no longer interact with the app.

### Multi-monitor

For v1:

- save last window position
- restore it if still valid
- if previous display is unavailable, move window to a visible region on the primary display

---

## 8. System tray / menu

Minimum menu:

```text
Tapkin
────────────
Skin
  <current skin>
  Reload Skin
  Open Skin Folder
────────────
Always on Top      ✓
Click Through
Lock Position
────────────
Settings…
Quit
```

If launch-at-login is included:

```text
Launch at Login
```

Use native menu/tray capabilities through Tauri where possible.

---

## 9. Settings persistence

Persist locally:

```rust
struct AppSettings {
    selected_skin: PathBuf,
    window_position: Option<Position>,
    window_size: Size,
    always_on_top: bool,
    click_through: bool,
    lock_position: bool,
    launch_at_login: bool,
}
```

Use a human-readable local format such as TOML or JSON.

Settings corruption must fall back to sane defaults.

---

## 10. Suggested repository layout

```text
tapkin/
├── PLAN.md
├── README.md
├── LICENSE
├── package.json
├── src/
│   ├── index.html
│   ├── main.ts
│   └── style.css
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── app.rs
│       ├── config.rs
│       ├── skin.rs
│       ├── state.rs
│       ├── window.rs
│       └── input/
│           ├── mod.rs
│           ├── rdev_backend.rs
│           ├── macos.rs
│           └── windows.rs
├── skins/
│   └── example/
│       ├── pet.toml
│       ├── idle.png
│       ├── typing_1.png
│       └── typing_2.png
└── tests/
```

Do not over-engineer modules if a simpler layout is clearer.

---

## 11. Frontend responsibilities

Keep frontend logic intentionally thin.

Responsibilities:

- render current pet image
- receive state/image-change events from Rust
- apply image URL
- expose drag region if required
- settings UI

Do not put keyboard-state logic in JavaScript.

Do not poll the backend.

Preferred flow:

```text
global input
    ↓
Rust InputBackend
    ↓
Rust state machine
    ↓
Tauri event
    ↓
frontend swaps <img src>
```

---

## 12. Backend responsibilities

Rust owns:

- global input capture
- privacy-sensitive event normalization
- state machine
- timing
- skin loading/validation
- settings persistence
- window state
- platform permission/error handling

The frontend should never need raw keyboard events.

---

## 13. Implementation phases

### Phase 1 — Skeleton

Create a Tauri 2 project that:

- builds on macOS
- opens a transparent frameless window
- displays a bundled PNG
- supports always-on-top
- can quit cleanly

Acceptance:

- `cargo tauri dev` launches pet window successfully on macOS.

### Phase 2 — Skin loader

Implement:

- `pet.toml`
- skin folder loading
- validation
- idle image
- typing frame list
- clear errors

Acceptance:

- replacing the sample skin folder changes the pet without code changes.

### Phase 3 — Local test input

Before global hooks, add a temporary local input path to validate animation.

Acceptance:

- pressing keys while pet/settings window has focus alternates typing frames and returns to idle.

Remove or hide test-only code before release.

### Phase 4 — Global keyboard backend

Implement platform-neutral input interface.

Start with `rdev`.

Acceptance on macOS:

- typing in Safari/TextEdit/Terminal animates pet.

Acceptance on Windows:

- typing in Notepad/browser animates pet.

### Phase 5 — Timing/state machine

Implement:

- alternating frames
- debounce/sane repeated key handling
- idle timeout
Unit-test state transitions independently from global hooks.

### Phase 6 — Desktop usability

Implement:

- drag
- size
- persisted position
- click-through
- lock position
- tray/menu
- reload skin
- open skin folder

### Phase 7 — Permissions and errors

macOS:

- detect missing global-input permission
- show actionable instructions
- retry listener

Windows:

- handle hook startup failure
- no silent failure

### Phase 8 — Packaging

Produce:

- macOS `.app`
- macOS `.dmg` if practical
- Windows `.exe` / installer supported by Tauri

Document unsigned-development build warnings if releases are not code-signed.

Do not block first functional v1 on paid Apple/Windows signing infrastructure.

---

## 14. Tests

### Rust unit tests

Test skin config:

- valid minimal config
- missing idle image
- empty typing list
- missing optional reaction
- malformed TOML

Test state machine:

```text
Idle + key       -> Typing(frame 0)
Typing + key     -> Typing(frame 1)
Typing + key     -> Typing(frame 0)
Typing + timeout -> Idle
```

Test settings:

- serialize / deserialize
- invalid settings fallback
- position validation

### Manual integration matrix

#### macOS

Test:

- Apple Silicon
- current supported macOS
- TextEdit
- Safari or Chrome
- Terminal
- VS Code
- multiple Spaces/desktops
- second monitor if available
- Input Monitoring permission denied
- permission granted after first denial
- sleep/wake
- app relaunch

#### Windows

Test:

- Windows 11
- Notepad
- Chrome/Edge
- VS Code
- multi-monitor if available
- sleep/wake
- app relaunch

---

## 15. Performance targets

This app should be effectively idle when the user is not typing.

Targets:

- no render loop while static
- no continuous high-frequency polling
- no animation timer unless needed
- image assets cached
- negligible CPU at idle
- reasonable memory use for a Tauri app

Avoid adding a game engine.

---

## 16. Accessibility / keyboard permission messaging

Suggested wording concept:

> Tapkin needs permission to detect when a key is pressed while other apps are active. It does not record or save what you type.

Keep this statement accurate to the implementation.

If the implementation ever starts tracking actual key values, update both behavior and documentation before release.

---

## 17. Build and development commands

Expected workflow should be documented in README.

Typical commands:

```bash
npm install
npm run tauri dev
npm run tauri build
```

Also ensure Rust-only tests can run with:

```bash
cd src-tauri
cargo test
```

CI should at minimum run:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

and frontend checks appropriate to the chosen minimal toolchain.

---

## 18. CI

Use GitHub Actions.

Required:

- Rust format check
- Clippy
- Rust tests
- frontend build
- Tauri build smoke test where practical

Recommended matrix:

```text
macos-latest
windows-latest
```

Do not attempt Linux support in v1 merely because CI offers Linux runners.

---

## 19. Release definition

v1.0 is ready when all of the following are true:

- macOS app launches and displays transparent pet.
- Windows app launches and displays transparent pet.
- User can load a custom skin folder.
- `idle.png` is displayed while inactive.
- global typing alternates between at least two typing frames.
- pet returns to idle after timeout.
- typing works while another application is focused.
- no typed text is stored or transmitted.
- macOS permission failure is clearly explained.
- window can be moved.
- window size and position persist.
- click-through can be toggled safely.
- app can always be quit from tray/menu.
- release build succeeds on both target platforms.
- README documents install, permissions, skin format, and known limitations.

---

## 20. Explicit v1 non-goals

Do not expand scope without a concrete bug/requirement.

Do not implement:

- Linux
- Wayland
- mobile
- Live2D
- Spine
- VRM
- layered hand/body compositing
- animation timeline editor
- online skin browser
- automatic updates
- accounts
- telemetry
- cloud storage
- keystroke statistics
- words-per-minute tracking
- text capture
- clipboard capture
- mouse tracking unless separately approved
- complex scripting

---

## 21. Coding guidelines for Codex

When implementing this plan:

1. Prefer the smallest maintainable implementation.
2. Keep platform-specific code isolated.
3. Do not introduce dependencies without a clear need.
4. Do not weaken macOS/Windows security settings to make global input work.
5. Do not log raw key values.
6. Keep the UI minimal.
7. Add tests for the state machine before adding extra features.
8. Treat skin files as untrusted input:
   - validate paths,
   - handle missing files,
   - avoid path traversal outside the selected skin directory where practical.
9. Handle listener failure explicitly.
10. Do not silently fall back to a mode that looks functional but is not receiving global input.
11. Run formatting, linting, tests, and production builds before considering a milestone complete.
12. If a platform API blocks progress, document the exact failure and implement behind the existing abstraction rather than rewriting unrelated code.

---

## 22. First Codex task

Start with **Phase 1 + Phase 2 only**.

Deliver:

- initialized Tauri 2 repository
- minimal transparent always-on-top pet window
- minimal frontend
- Rust skin loader
- bundled example skin
- `pet.toml` parsing and validation
- unit tests for skin configuration
- README with development commands

Do **not** implement global keyboard monitoring until the shell and skin format are working.

After Phase 1 + 2 pass, continue with Phase 3 and Phase 4.

---

## 23. Product principle

Tapkin should feel like a tiny desktop toy, not a framework.

The architecture may be extensible, but v1 should remain:

- small
- local
- private
- responsive
- skin-driven
- easy to understand
- easy to build
