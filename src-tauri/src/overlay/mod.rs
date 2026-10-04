//! The floating pet only. Settings, menus and application lifecycle belong to the shell.
use crate::window::{Position, Size};
use thiserror::Error;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod tauri;

pub type OverlayResult<T = ()> = Result<T, OverlayError>;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("The pet overlay is unavailable")]
    WindowUnavailable,
    #[error("Invalid overlay frame index: {0}")]
    InvalidFrame(usize),
    #[error("Overlay {operation} failed: {message}")]
    OperationFailed {
        operation: &'static str,
        message: String,
    },
}

/// Validated, cached PNG bytes, independent of a file URL, WebView or native image API.
/// The identifiers let a renderer cache images and discard stale presentation requests.
pub struct FrameRef<'a> {
    pub png: &'a [u8],
    pub name: &'a str,
    pub index: usize,
    pub revision: u64,
    pub sequence: u64,
}

pub trait OverlayRenderer: Send + Sync {
    fn show_frame(&self, frame: &FrameRef<'_>) -> OverlayResult;
    fn set_visible(&self, visible: bool) -> OverlayResult;
    fn set_always_on_top(&self, enabled: bool) -> OverlayResult;
    fn set_click_through(&self, enabled: bool) -> OverlayResult;
    /// Physical desktop pixels, including negative monitor coordinates.
    fn set_position(&self, position: Position) -> OverlayResult;
    /// Logical window pixels; the caller has already preserved the skin's aspect ratio.
    fn set_size(&self, size: Size) -> OverlayResult;
    /// Validate a saved physical position against current displays, then apply it.
    fn restore_position(&self, saved: Option<Position>) -> OverlayResult<Position>;
    fn current_position(&self) -> OverlayResult<Position>;
    fn start_dragging(&self) -> OverlayResult;
}
