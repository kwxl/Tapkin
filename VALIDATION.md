# v1 implementation and validation

The v1 feature set is implemented. **Native release acceptance is still pending**: this workspace runs Linux, so it cannot launch a macOS/Windows desktop or validate OS permissions. A cross-check is not a linked app or an installer.

## Completed checks in this workspace

- `npm ci`, `npm run check`, `npm run build`: passed.
- `npm audit`: no reported vulnerabilities.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed for the portable core.
- `cargo test --locked`: **16 tests passed** on Linux. Symlink-confinement tests also ran; the Unix-only image-symlink test is omitted on Windows.
- Entire desktop Rust code, including Tauri command/context generation and platform hooks: Clippy with `--all-targets -- -D warnings` passed for **`x86_64-pc-windows-gnu`** and **`aarch64-apple-darwin`**.
- `cargo check --release` passed for both of those desktop targets, including the production frontend embedded into the Tauri context.
- Headless Chromium smoke test of the production frontend: settings rendering, permission retry UI, click-through setting changes, frame events, stale-event rejection, invalid-reload errors, privacy shortcut invocation, transparent pet DOM and drag/settings command invocation passed with no browser exceptions. **Tauri IPC was mocked**; this does not validate a native tray, pointer pass-through or global keyboard hook.

For cross-checks only, the workspace used an extracted MinGW windres with the host C preprocessor and an extracted Debian Clang for Apple's Objective-C dependency. No dependency sources or security settings were modified. Initial macOS checks failed because host GCC does not support `-arch` / `-mmacosx-version-min`; Clang resolved the code-check limitation. These temporary toolchain helpers are not part of Tapkin.

The GitHub Actions workflow is provided as the inactive template `ci/desktop.yml` and has not been executed. Initial repository upload credentials do not permit active workflow registration. Enable the template by moving it to `.github/workflows/desktop.yml` with workflow write permission. It runs the native tests and creates `.app`/`.dmg` and Windows NSIS installer artifacts on the destination operating systems. No signed or unsigned installer has been produced locally, and no release has been published.

## Manual acceptance before calling v1.0 release-ready

### macOS

- [ ] Run the workflow / `npm run tauri build -- --bundles app,dmg` on a Mac and launch the installed `.app`.
- [ ] Confirm transparency, no decorations, always-on-top and no focus theft.
- [ ] On Apple Silicon and the supported current macOS, type in TextEdit, Safari/Chrome, Terminal and VS Code: typing frames alternate and return to idle.
- [ ] Deny Input Monitoring: Settings explains the failure and reports an inactive listener.
- [ ] Grant permission, retry, and confirm actual input. If the OS requests app relaunch, verify relaunch recovery.
- [ ] Check drag, aspect-preserving resize, reload, a different custom skin, invalid skin rejection, settings corruption and app relaunch.
- [ ] Enable Click Through, verify pointer pass-through, then turn it off from the tray without restarting.
- [ ] Check Lock Position, Quit, Launch at Login, sleep/wake and multiple Spaces.
- [ ] With a second display, check negative coordinates / mixed DPI, disconnect it and relaunch to verify visible position recovery.
- [ ] Confirm low idle CPU in Activity Monitor; no periodic frame swaps without key presses.

### Windows

- [ ] Run the workflow / `npm run tauri build -- --bundles nsis` on Windows and install/launch the generated installer.
- [ ] On Windows 11, verify transparency, topmost behavior and no focus theft.
- [ ] Type in Notepad, Chrome/Edge and VS Code; check alternating frames and idle timeout.
- [ ] Check the tray in the notification-area overflow, click-through recovery and Quit.
- [ ] Check skin selection/reload/errors, drag, aspect-preserving resize, position persistence, lock and login startup.
- [ ] Check sleep/wake, app relaunch and multiple monitors including mixed DPI and disconnected-display recovery.
- [ ] Confirm low idle CPU in Task Manager.

v1 intentionally has no Linux desktop backend, network features, key mappings, text history, reactions, audio, updater or animation editor. Secure input fields and exclusive full-screen windows may restrict hooks/overlays; the implementation does not bypass OS protections.
