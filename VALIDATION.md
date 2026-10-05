# v1 implementation and validation

Documentation reviewed on **2026-10-05** against code commit **`6dda6c5d7db07f918fa6b4eded0a8252b8c55f7d`**. The current implementation includes pnpm tooling, the square-headed typing tabby skin/icons and live size previews through `preview_size`.

**Native release acceptance is still pending.** Native macOS/Windows builds and installer uploads have succeeded in CI; launching the installed app and checking desktop interactions, keyboard permissions and display behavior still require manual acceptance. This Linux workspace cannot perform those GUI checks.

## Current native CI evidence

[Desktop checks and installers — run 37245677740](https://github.com/kwxl/Tapkin/actions/runs/37245677740) ran for `6dda6c5` on 2026-10-04/05 UTC. GitHub reports the run and both jobs as **completed / success**; job steps and uploaded-artifact metadata were inspected during this documentation review.

| Runner / job | Passed checks and build | Uploaded artifact |
| --- | --- | --- |
| [macos-latest](https://github.com/kwxl/Tapkin/actions/runs/37245677740/job/111562989964) | `pnpm ci`, frontend build/type check, Rust formatting, Clippy, tests, `pnpm run tauri build --bundles app,dmg` and artifact upload | `tapkin-macos` (DMG) |
| [windows-latest](https://github.com/kwxl/Tapkin/actions/runs/37245677740/job/111562989850) | `pnpm ci`, frontend build/type check, Rust formatting, Clippy, tests, `pnpm run tauri build --bundles nsis` and artifact upload | `tapkin-windows` (NSIS EXE) |

The workflow is `.github/workflows/desktop.yml`, triggered by pushes to `main`, pull requests and manual dispatch. It uses `pnpm/setup@v3` with `runtime: node@22`, pnpm caching and the package's pinned pnpm 12.9.1. Rust checks use `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked`. Artifacts are retained for 14 days. The macOS build creates an `.app` as well as a DMG, but only the DMG is uploaded. No signing/notarization or release-publishing step is configured.

The artifacts were reported present and unexpired when reviewed. They were not downloaded, installed or launched here. Successful build jobs do not verify transparency, pointer pass-through, a live keyboard hook or OS permission recovery.

This update changes documentation only. It did not rerun local code tests, regenerate a lockfile or rebuild the app. The following earlier checks remain recorded against their original revision.

## Earlier cloud checks at `5ee8993`

These checks predate the pnpm migration, live resize and updated artwork. The npm install/audit and mocked browser results describe that earlier revision, not a new validation of the current lockfile or slider.

- `npm ci`, `npm run check`, `npm run build`: passed.
- `npm audit`: no reported vulnerabilities.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed for the portable core.
- `cargo test --locked`: **22 tests passed** (16 existing unit tests and 6 new non-Tauri overlay contract tests) on Linux. Symlink-confinement tests also ran; the Unix-only image-symlink test is omitted on Windows.
- Entire desktop Rust code, including Tauri command/context generation and platform hooks: Clippy with `--all-targets -- -D warnings` passed for **`x86_64-pc-windows-gnu`** and **`aarch64-apple-darwin`**.
- `cargo check --release` passed for both of those desktop targets, including the production frontend embedded into the Tauri context.
- Headless Chromium smoke test of the production frontend: settings rendering, permission retry UI, click-through setting changes, frame events, stale-event rejection, independent overlay skin revisions, first-use PNG/cached frame swaps, Settings skin/preview events, invalid-reload errors, privacy shortcut invocation, transparent pet DOM and drag/settings command invocation passed with no browser exceptions. **Tauri IPC was mocked**; this does not validate a native tray, pointer pass-through or global keyboard hook.

For cross-checks only, the workspace used an extracted MinGW windres with the host C preprocessor and an extracted Debian Clang for Apple's Objective-C dependency. No dependency sources or security settings were modified. Initial macOS checks failed because host GCC does not support `-arch` / `-mmacosx-version-min`; Clang resolved the code-check limitation. These temporary toolchain helpers are not part of Tapkin.

## Overlay extraction coverage

The fake renderer in `src-tauri/tests/overlay_contract.rs` runs the real injected `TapkinApp` without a Tauri window. It covers frame alternation/hold/idle/reset, immutable cached PNG bytes, overlay settings and rollback, skin revisions and failed replacement, movement save deadlines, shutdown position capture, and lock/click-through drag guards. The six tests run with `cargo test --locked --test overlay_contract`.

The current code retains the native hook implementations, settings schema and event-driven image swaps. The newer size slider uses transient `preview_size` calls, followed by a persistent `update_settings` call when the adjustment completes; previews do not save settings. The bundled skin and icons now use the typing tabby artwork, while existing writable example files are preserved. See [ARCHITECTURE.md](ARCHITECTURE.md) for the dependency boundary. Native transparency, hit testing, tray, dragging, monitor/DPI behavior and keyboard permissions remain pending the desktop checks below.

## Manual acceptance before calling v1.0 release-ready

### macOS

- [x] Build `.app`/DMG on macOS in the linked CI run.
- [ ] Install/launch that build on a Mac, or build locally with `pnpm run tauri build --bundles app,dmg`.
- [ ] Confirm transparency, no decorations, always-on-top and no focus theft.
- [ ] On Apple Silicon and the supported current macOS, type in TextEdit, Safari/Chrome, Terminal and VS Code: typing frames alternate and return to idle.
- [ ] Deny Input Monitoring: Settings explains the failure and reports an inactive listener.
- [ ] Grant permission, retry, and confirm actual input. If the OS requests app relaunch, verify relaunch recovery.
- [ ] Check drag, live aspect-preserving resize while moving the slider, final size persistence after release/keyboard adjustment, failed-preview recovery, reload, a different custom skin, invalid skin rejection, settings corruption and app relaunch.
- [ ] Enable Click Through, verify pointer pass-through, then turn it off from the tray without restarting.
- [ ] Check Lock Position, Quit, Launch at Login, sleep/wake and multiple Spaces.
- [ ] With a second display, check negative coordinates / mixed DPI, disconnect it and relaunch to verify visible position recovery.
- [ ] Confirm low idle CPU in Activity Monitor; no periodic frame swaps without key presses.

### Windows

- [x] Build/upload a Windows NSIS installer in the linked CI run.
- [ ] Install/launch that build on Windows, or build locally with `pnpm run tauri build --bundles nsis`.
- [ ] On Windows 11, verify transparency, topmost behavior and no focus theft.
- [ ] Type in Notepad, Chrome/Edge and VS Code; check alternating frames and idle timeout.
- [ ] Check the tray in the notification-area overflow, click-through recovery and Quit.
- [ ] Check skin selection/reload/errors, drag, live aspect-preserving resize, final size persistence after release/keyboard adjustment, failed-preview recovery, position persistence, lock and login startup.
- [ ] Check sleep/wake, app relaunch and multiple monitors including mixed DPI and disconnected-display recovery.
- [ ] Confirm low idle CPU in Task Manager.

v1 intentionally has no Linux desktop backend, network features, key mappings, text history, reactions, audio, updater or animation editor. Secure input fields and exclusive full-screen windows may restrict hooks/overlays; the implementation does not bypass OS protections.
