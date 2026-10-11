#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn main() {
    #[cfg(target_os = "linux")]
    tapkin_core::linux::prepare_environment();
    tapkin_core::app::run();
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn main() {
    eprintln!("Tapkin v1 supports macOS, Windows and Linux only. Core unit tests can run here.");
    std::process::exit(1);
}
