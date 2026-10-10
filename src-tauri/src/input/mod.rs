//! Hooks normalize physical key identities for transient image selection in Rust.
//! Characters are transient mapping inputs; no text or key history reaches the frontend.
use std::sync::Arc;

mod keys;
pub use keys::KeyCode;

#[derive(Clone, Copy)]
pub enum InputEvent {
    ResetKeys,
    KeyDown {
        id: u32,
        key: Option<KeyCode>,
        character: Option<char>,
        repeat: bool,
    },
    KeyUp {
        id: u32,
    },
    AnyKeyPressed,
    KeyPressed(KeyCode),
    CharacterPressed {
        key: Option<KeyCode>,
        character: char,
    },
}

/// Accept exactly one printable Unicode scalar, never a string or text history.
pub(crate) fn single_character(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let character = chars.next()?;
    (!character.is_control() && chars.next().is_none()).then_some(character)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(super) fn character_from_utf16(units: &[u16]) -> Option<char> {
    let mut chars = char::decode_utf16(units.iter().copied());
    let character = chars.next()?.ok()?;
    (!character.is_control() && chars.next().is_none()).then_some(character)
}

#[cfg(test)]
pub(super) fn key_event(key: Option<KeyCode>, character: Option<char>) -> InputEvent {
    match character {
        Some(character) => InputEvent::CharacterPressed { key, character },
        None => key.map_or(InputEvent::AnyKeyPressed, InputEvent::KeyPressed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_single_printable_scalars_only() {
        for text in ["?", "!", "a", "A", "é", "😀", " "] {
            assert_eq!(single_character(text), text.chars().next());
            assert_eq!(
                character_from_utf16(&text.encode_utf16().collect::<Vec<_>>()),
                text.chars().next()
            );
        }
        for text in ["", "ab", "\n", "\t", "e\u{301}"] {
            assert_eq!(single_character(text), None);
            assert_eq!(
                character_from_utf16(&text.encode_utf16().collect::<Vec<_>>()),
                None
            );
        }
        assert_eq!(character_from_utf16(&[0xd800]), None);
    }

    #[test]
    fn character_events_preserve_physical_fallback() {
        let key = KeyCode::parse("Slash").unwrap();
        assert!(
            matches!(key_event(Some(key), Some('?')), InputEvent::CharacterPressed { key: Some(k), character: '?' } if k == key)
        );
        assert!(matches!(key_event(Some(key), None), InputEvent::KeyPressed(k) if k == key));
        assert!(matches!(key_event(None, None), InputEvent::AnyKeyPressed));
    }
}

pub type ActivitySink = Arc<dyn Fn(InputEvent) + Send + Sync>;
pub type FailureSink = Arc<dyn Fn(String) + Send + Sync>;

pub trait InputListener: Send + Sync {}
pub trait InputBackend: Send + Sync {
    /// Returns only after the OS has accepted the hook. Dropping the listener stops it.
    fn start(
        &self,
        activity: ActivitySink,
        failure: FailureSink,
    ) -> Result<Box<dyn InputListener>, String>;
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn platform_backend() -> Box<dyn InputBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacInput)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsInput)
    }
}
