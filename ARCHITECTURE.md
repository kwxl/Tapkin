# Tapkin architecture

Tauri remains the application shell and the Settings UI. Only the floating pet is replaceable through `OverlayRenderer`; `TauriOverlayRenderer` is the current production implementation.

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

The settings HTML/CSS and controls remain unchanged. The shared frontend's small overlay-event adapter is an implementation detail of the current renderer, while Settings retains its normal Tauri integration.

## Remaining Tauri dependencies

| Location | Reason |
| --- | --- |
| `overlay/tauri.rs` | Own/create the transparent pet window, emit its frame events, apply topmost/click-through/drag/geometry and monitor recovery. |
| `app.rs` | Bootstrap/inject the renderer, manage normal Settings windows, IPC commands, tray/menu, app data/resource paths, login integration and lifecycle. |
| `src/main.ts` | Existing Tauri Settings commands/dialogs/events and the current WebView overlay adapter. |
| `build.rs`, Tauri config/capabilities and shell plugins | Existing app packaging, permissions and platform integration. |

A future macOS/Windows native overlay implements this same contract and is constructed/injected at bootstrap. Skin parsing, frame selection, input normalization, settings schema and Settings UI can remain as they are. Its OS-specific image/window/event-loop setup belongs inside that implementation. There is no native renderer, registry or runtime switching in this change.

## Preserved behavior and verification

The inspected v1 baseline uses a transparent, frameless, unfocusable pet WebView with cached PNG swaps, alternating typing frames, frame hold and idle timeout. It supports topmost, click-through, dragging/lock, aspect-preserving resize, saved position/size and monitor recovery; Settings, skin selection/reload, tray controls, login startup and shutdown remain in the shell. Existing native hooks and bundled artwork are unchanged. Linux cloud checks cannot establish native desktop behavior; the platform acceptance checklist is in [VALIDATION.md](VALIDATION.md).

`tests/overlay_contract.rs` implements a simple non-Tauri renderer and exercises the actual `TapkinApp` path: frame alternation/hold/idle/reset, cached PNG bytes, settings application/rollback, skin revision/reload failure, position save deadlines/shutdown capture and drag lock/click-through. Existing parsing, validation, settings, state and monitor tests remain.

```sh
cd src-tauri
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --test overlay_contract
```

On Linux these tests compile and run without Tauri windows or a desktop backend. Cross-target code checks and the mocked production-frontend smoke test complement them; neither replaces native macOS/Windows acceptance.
