# Tapkin architecture

This document describes the code reviewed at `6dda6c5` on 2026-10-05. Tauri remains the application shell and the Settings UI. Only the floating pet is replaceable through `OverlayRenderer`; `TauriOverlayRenderer` is the current production implementation.

```text
Tapkin
├── Tauri App Shell (app.rs)
│   ├── Settings WebView / commands / dialogs
│   ├── Tray / Menu
│   ├── Startup / shutdown / launch at login
│   └── constructs TauriOverlayRenderer (overlay/tauri.rs)
└── Core
    ├── TapkinApp (core.rs): injected Arc<dyn OverlayRenderer>
    ├── InputBackend (input/): native hooks → AnyKeyPressed
    ├── SkinManager responsibility (skin.rs): parse / validate / cache PNGs
    ├── SettingsStore responsibility (config.rs): validate / load / save
    ├── Animation (state.rs): select frames / hold / idle timeout
    ├── Geometry (window.rs): aspect ratio / display recovery
    └── OverlayRenderer contract (overlay/mod.rs)
```

Existing `skin.rs` and `config.rs` serve the skin/settings responsibilities; no duplicate managers or new crates were introduced. `core.rs`, `state.rs`, `skin.rs`, `config.rs`, `window.rs`, the input abstraction and the overlay contract use no Tauri window types. Core receives the renderer from bootstrap and never fetches a pet window globally. The concrete implementation owns the pet `WebviewWindow` and all its operations.

## Overlay contract

```rust
pub trait OverlayRenderer: Send + Sync {
    fn show_frame(&self, frame: &FrameRef<'_>) -> OverlayResult;
    fn set_visible(&self, visible: bool) -> OverlayResult;
    fn set_always_on_top(&self, enabled: bool) -> OverlayResult;
    fn set_click_through(&self, enabled: bool) -> OverlayResult;
    fn set_position(&self, position: Position) -> OverlayResult;
    fn set_size(&self, size: Size) -> OverlayResult;
    fn restore_position(&self, saved: Option<Position>) -> OverlayResult<Position>;
    fn current_position(&self) -> OverlayResult<Position>;
    fn start_dragging(&self) -> OverlayResult;
}
```

`OverlayResult<T = ()>` wraps project-owned `OverlayError`: unavailable window, invalid frame index, or an operation name and diagnostic message. No Tauri error type crosses this boundary.

`FrameRef` borrows the skin's validated, cached PNG bytes, with a name, frame index, skin revision and monotonic presentation sequence. Editing files on disk does not change those bytes until a successful reload. A renderer can create its own native image or WebView representation without knowing the skin's file layout. `Position` is an existing project-owned pair of signed physical desktop pixel coordinates, including negative monitor coordinates. `Size` uses logical window pixels. Display/DPI conversion and saved-position recovery are renderer responsibilities; aspect ratio calculation remains shared pure logic. `restore_position` and `start_dragging` represent existing behavior; unused size queries were not added.

## Presentation and settings flow

The existing native input hooks emit only `AnyKeyPressed` to the existing bounded engine channel. They do not call a window API. `TapkinApp` drives the existing animation state machine, requests `show_frame`, and returns neutral frame metadata for the shell's Settings preview. Idle remains event/deadline driven, without a polling render loop.

`TauriOverlayRenderer` sends a targeted `pet-frame` event to the pet WebView. It encodes/sends each PNG once per skin revision, then emits small frame metadata for subsequent swaps. The existing frontend continues to cache images and update the `<img>` source. Its initial IPC snapshot also supplies all cached images, covering events sent before listeners are ready and WebView reloads.

The shell separately sends skin/frame/settings/status events to the Settings WebView. A successful skin replacement renders the new idle frame through the renderer before committing the new core skin/state. Failed validation leaves the current skin intact; renderer/save errors retain diagnostics and restore prior settings/geometry where possible. Settings/tray commands change overlay properties through the contract. Geometry calls run outside the shared core mutex because a renderer may dispatch to its UI thread. Movement is queued back to the engine for the existing 250 ms debounced position save; shutdown reads the actual renderer position before saving.

The Settings frontend remains Tauri-based. Its size slider now has a separate transient preview path:

```text
slider input → coalesced preview_size(width) → OverlayRenderer::set_size
slider change → update_settings({ width }) → apply settings / restore position / save / notify
```

`src/main.ts` updates the displayed dimensions immediately, coalesces pending widths and limits preview requests to about 30 per second, with one resize operation in flight. A final commit is queued behind an in-flight preview. `preview_size` validates a finite width in 64–800, applies the shared `aspect_size` calculation and calls the injected renderer outside the core-state mutex. It does not update `AppSettings`, save JSON, restore position or emit a settings event. `update_settings` performs those persistent operations when the slider change completes. A failed preview reports an error and queues a restore to the saved width when no newer resize is pending.

The shared frontend's overlay-event adapter remains an implementation detail of the current renderer. Live resizing is a shell command using the existing `set_size` contract; it adds no renderer method or core animation state.

## Remaining Tauri dependencies

| Location | Reason |
| --- | --- |
| `overlay/tauri.rs` | Own/create the transparent pet window, emit its frame events, apply topmost/click-through/drag/geometry and monitor recovery. |
| `app.rs` | Bootstrap/inject the renderer, manage normal Settings windows, persistent settings and transient `preview_size` commands, tray/menu, app data/resource paths, login integration and lifecycle. |
| `src/main.ts` | Existing Tauri Settings commands/dialogs/events and the current WebView overlay adapter. |
| `build.rs`, Tauri config/capabilities and shell plugins | Existing app packaging, permissions and platform integration. |

A future macOS/Windows native overlay implements this same contract and is constructed/injected at bootstrap. Skin parsing, frame selection, input normalization, settings schema and Settings UI can remain as they are. Its OS-specific image/window/event-loop setup belongs inside that implementation. There is no native renderer, registry or runtime switching in this change.

## Preserved behavior and verification

The current implementation uses a transparent, frameless, unfocusable pet WebView with cached PNG swaps, alternating typing frames, frame hold and idle timeout. It supports topmost, click-through, dragging/lock, aspect-preserving resize, saved position/size and monitor recovery; Settings, skin selection/reload, tray controls, login startup and shutdown remain in the shell. The native hooks still emit only generic typing activity. The bundled skin and application icons now use the square-headed typing tabby pixel cat. `install_example` preserves existing files, so an existing writable example copy is not replaced by an artwork update. Native desktop interaction still requires the platform acceptance checklist in [VALIDATION.md](VALIDATION.md), which also records successful macOS/Windows CI builds.

`tests/overlay_contract.rs` implements a simple non-Tauri renderer and exercises the actual `TapkinApp` path: frame alternation/hold/idle/reset, cached PNG bytes, settings application/rollback, skin revision/reload failure, position save deadlines/shutdown capture and drag lock/click-through. Existing parsing, validation, settings, state and monitor tests remain.

```sh
cd src-tauri
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --test overlay_contract
```

On Linux these tests compile and run without Tauri windows or a desktop backend. The current GitHub Actions workflow uses pnpm 12.9.1 and Node 22 to build the frontend, then runs Rust checks/tests and Tauri installer builds on macOS and Windows. Those builds and the earlier mocked frontend smoke test do not establish native transparency, input permissions or desktop interaction; see [VALIDATION.md](VALIDATION.md) for evidence and pending manual checks.
