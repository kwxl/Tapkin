fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target == "macos" || target == "windows" || target == "linux" {
        tauri_build::build();
    }
}
