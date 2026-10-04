//! Platform hooks discard native event payloads at the boundary.
//! No character, key code, modifier state, or typed text leaves these modules.
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum InputEvent {
    AnyKeyPressed,
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
