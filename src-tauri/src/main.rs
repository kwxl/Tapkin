#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    tapkin_core::app::run();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("Tapkin v1 supports macOS and Windows only. Core unit tests can run here.");
    std::process::exit(1);
}
