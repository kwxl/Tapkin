//! Linux desktop integration that must run before Tauri/GTK initializes.

/// Prefer the X11 GDK backend, including XWayland on Wayland sessions.
///
/// Native Wayland does not let applications position their own windows, keep them
/// above others or start interactive moves the way the pet overlay requires. Under
/// XWayland the existing Tauri overlay keeps the macOS/Windows behavior: saved
/// positions, dragging, always-on-top and click-through. When no X server is
/// available, the default backend is left untouched so the app can still start.
pub fn prepare_environment() {
    if std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty()) {
        // Called from `main` before any other thread exists.
        std::env::set_var("GDK_BACKEND", "x11");
    }
}
