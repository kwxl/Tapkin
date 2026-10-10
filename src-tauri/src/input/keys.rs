//! Physical key positions, using UI Events `code` names, not typed characters.
//! Modifier-only, media, and vendor-specific keys are intentionally not mapped.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyCode(&'static str);

// Name, macOS virtual keycode, Windows scan code (0xe000 marks extended).
const KEYS: &[(&str, u32, u32)] = &[
    ("KeyA", 0, 0x1e),
    ("KeyB", 11, 0x30),
    ("KeyC", 8, 0x2e),
    ("KeyD", 2, 0x20),
    ("KeyE", 14, 0x12),
    ("KeyF", 3, 0x21),
    ("KeyG", 5, 0x22),
    ("KeyH", 4, 0x23),
    ("KeyI", 34, 0x17),
    ("KeyJ", 38, 0x24),
    ("KeyK", 40, 0x25),
    ("KeyL", 37, 0x26),
    ("KeyM", 46, 0x32),
    ("KeyN", 45, 0x31),
    ("KeyO", 31, 0x18),
    ("KeyP", 35, 0x19),
    ("KeyQ", 12, 0x10),
    ("KeyR", 15, 0x13),
    ("KeyS", 1, 0x1f),
    ("KeyT", 17, 0x14),
    ("KeyU", 32, 0x16),
    ("KeyV", 9, 0x2f),
    ("KeyW", 13, 0x11),
    ("KeyX", 7, 0x2d),
    ("KeyY", 16, 0x15),
    ("KeyZ", 6, 0x2c),
    ("Digit0", 29, 0x0b),
    ("Digit1", 18, 0x02),
    ("Digit2", 19, 0x03),
    ("Digit3", 20, 0x04),
    ("Digit4", 21, 0x05),
    ("Digit5", 23, 0x06),
    ("Digit6", 22, 0x07),
    ("Digit7", 26, 0x08),
    ("Digit8", 28, 0x09),
    ("Digit9", 25, 0x0a),
    ("Space", 49, 0x39),
    ("Enter", 36, 0x1c),
    ("Tab", 48, 0x0f),
    ("Escape", 53, 0x01),
    ("Backspace", 51, 0x0e),
    ("Delete", 117, 0xe053),
    ("ArrowLeft", 123, 0xe04b),
    ("ArrowRight", 124, 0xe04d),
    ("ArrowUp", 126, 0xe048),
    ("ArrowDown", 125, 0xe050),
    ("Home", 115, 0xe047),
    ("End", 119, 0xe04f),
    ("PageUp", 116, 0xe049),
    ("PageDown", 121, 0xe051),
    ("Minus", 27, 0x0c),
    ("Equal", 24, 0x0d),
    ("BracketLeft", 33, 0x1a),
    ("BracketRight", 30, 0x1b),
    ("Backslash", 42, 0x2b),
    ("Semicolon", 41, 0x27),
    ("Quote", 39, 0x28),
    ("Backquote", 50, 0x29),
    ("Comma", 43, 0x33),
    ("Period", 47, 0x34),
    ("Slash", 44, 0x35),
    ("F1", 122, 0x3b),
    ("F2", 120, 0x3c),
    ("F3", 99, 0x3d),
    ("F4", 118, 0x3e),
    ("F5", 96, 0x3f),
    ("F6", 97, 0x40),
    ("F7", 98, 0x41),
    ("F8", 100, 0x42),
    ("F9", 101, 0x43),
    ("F10", 109, 0x44),
    ("F11", 103, 0x57),
    ("F12", 111, 0x58),
    ("Numpad0", 82, 0x52),
    ("Numpad1", 83, 0x4f),
    ("Numpad2", 84, 0x50),
    ("Numpad3", 85, 0x51),
    ("Numpad4", 86, 0x4b),
    ("Numpad5", 87, 0x4c),
    ("Numpad6", 88, 0x4d),
    ("Numpad7", 89, 0x47),
    ("Numpad8", 91, 0x48),
    ("Numpad9", 92, 0x49),
    ("NumpadDecimal", 65, 0x53),
    ("NumpadAdd", 69, 0x4e),
    ("NumpadSubtract", 78, 0x4a),
    ("NumpadMultiply", 67, 0x37),
    ("NumpadDivide", 75, 0xe035),
    ("NumpadEnter", 76, 0xe01c),
];

impl KeyCode {
    pub fn parse(name: &str) -> Option<Self> {
        KEYS.iter()
            .find(|entry| entry.0 == name)
            .map(|entry| Self(entry.0))
    }

    #[cfg(any(target_os = "macos", test))]
    pub(super) fn from_macos(code: u32) -> Option<Self> {
        KEYS.iter()
            .find(|entry| entry.1 == code)
            .map(|entry| Self(entry.0))
    }

    #[cfg(any(target_os = "windows", test))]
    pub(super) fn from_windows(scan: u32, extended: bool) -> Option<Self> {
        let code = scan | if extended { 0xe000 } else { 0 };
        KEYS.iter()
            .find(|entry| entry.2 == code)
            .map(|entry| Self(entry.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_supported_keys_round_trip_without_collisions() {
        let mut names = std::collections::BTreeSet::new();
        let mut mac = std::collections::BTreeSet::new();
        let mut windows = std::collections::BTreeSet::new();
        for &(name, mac_code, win_code) in KEYS {
            assert!(names.insert(name));
            assert!(mac.insert(mac_code));
            assert!(windows.insert(win_code));
            let key = KeyCode::parse(name).unwrap();
            assert_eq!(KeyCode::from_macos(mac_code), Some(key));
            assert_eq!(
                KeyCode::from_windows(win_code & 0xff, win_code & 0xe000 != 0),
                Some(key)
            );
        }
    }

    #[test]
    fn rejects_unknown_keys_and_distinguishes_extended_keys() {
        for name in ["A", "keya", "ShiftLeft", "Ctrl+KeyA", ""] {
            assert_eq!(KeyCode::parse(name), None);
        }
        assert_eq!(KeyCode::from_macos(65535), None);
        assert_eq!(KeyCode::from_windows(0, false), None);
        assert_eq!(KeyCode::from_windows(0x1c, false), KeyCode::parse("Enter"));
        assert_eq!(
            KeyCode::from_windows(0x1c, true),
            KeyCode::parse("NumpadEnter")
        );
        assert_eq!(
            KeyCode::from_windows(0x48, false),
            KeyCode::parse("Numpad8")
        );
        assert_eq!(KeyCode::from_windows(0x48, true), KeyCode::parse("ArrowUp"));
    }
}
