//! The current WebView overlay. This is the only module that owns the pet window.
use super::{FrameRef, OverlayError, OverlayRenderer, OverlayResult};
use crate::window::{restore_position, DisplayRect, Position, Size};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter, PhysicalPosition, WebviewWindow, WindowEvent};

#[derive(Default)]
struct FrameCache {
    revision: Option<u64>,
    images: HashMap<usize, String>,
}

#[derive(Clone, Serialize)]
struct FrameEvent<'a> {
    revision: u64,
    frame: usize,
    sequence: u64,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<&'a str>,
}

pub struct TauriOverlayRenderer {
    app: AppHandle,
    window: OnceLock<WebviewWindow>,
    cache: Mutex<FrameCache>,
}

impl TauriOverlayRenderer {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            window: OnceLock::new(),
            cache: Mutex::new(FrameCache::default()),
        }
    }

    /// Bootstrap after the shell manages TapkinApp, so the WebView's initial IPC snapshot
    /// is available as soon as it loads. No renderer is kept in mutable global state.
    pub fn create_window(
        &self,
        size: Size,
        always_on_top: bool,
        on_moved: impl Fn(Position) + Send + Sync + 'static,
    ) -> OverlayResult {
        let window = tauri::WebviewWindowBuilder::new(
            &self.app,
            "pet",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Tapkin")
        .inner_size(size.width, size.height)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(always_on_top)
        .resizable(false)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| operation("create window", e))?;
        #[cfg(target_os = "macos")]
        window
            .set_visible_on_all_workspaces(true)
            .map_err(|e| operation("set workspaces", e))?;
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
            WindowEvent::Moved(position) => on_moved(Position {
                x: position.x,
                y: position.y,
            }),
            _ => {}
        });
        self.window
            .set(window)
            .map_err(|_| operation("create window", "overlay was already initialized"))
    }

    fn window(&self) -> OverlayResult<&WebviewWindow> {
        self.window.get().ok_or(OverlayError::WindowUnavailable)
    }
}

fn operation(operation: &'static str, error: impl std::fmt::Display) -> OverlayError {
    OverlayError::OperationFailed {
        operation,
        message: error.to_string(),
    }
}

impl OverlayRenderer for TauriOverlayRenderer {
    fn show_frame(&self, frame: &FrameRef<'_>) -> OverlayResult {
        if !frame.png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(OverlayError::InvalidFrame(frame.index));
        }
        let mut cache = self.cache.lock().map_err(|e| operation("cache frame", e))?;
        if cache.revision != Some(frame.revision) {
            cache.images.clear();
            cache.revision = Some(frame.revision);
        }
        // Send PNG data only on first use in a revision; normal key events stay tiny.
        let image = (!cache.images.contains_key(&frame.index))
            .then(|| format!("data:image/png;base64,{}", STANDARD.encode(frame.png)));
        self.window()?
            .emit_to(
                "pet",
                "pet-frame",
                FrameEvent {
                    revision: frame.revision,
                    frame: frame.index,
                    sequence: frame.sequence,
                    name: frame.name,
                    image: image.as_deref(),
                },
            )
            .map_err(|e| operation("show frame", e))?;
        if let Some(image) = image {
            cache.images.insert(frame.index, image);
        }
        Ok(())
    }

    fn set_visible(&self, visible: bool) -> OverlayResult {
        let window = self.window()?;
        if visible {
            window.show()
        } else {
            window.hide()
        }
        .map_err(|e| operation("set visibility", e))
    }
    fn set_always_on_top(&self, enabled: bool) -> OverlayResult {
        self.window()?
            .set_always_on_top(enabled)
            .map_err(|e| operation("set always-on-top", e))
    }
    fn set_click_through(&self, enabled: bool) -> OverlayResult {
        self.window()?
            .set_ignore_cursor_events(enabled)
            .map_err(|e| operation("set click-through", e))
    }
    fn set_position(&self, position: Position) -> OverlayResult {
        self.window()?
            .set_position(PhysicalPosition::new(position.x, position.y))
            .map_err(|e| operation("set position", e))
    }
    fn set_size(&self, size: Size) -> OverlayResult {
        self.window()?
            .set_size(tauri::LogicalSize::new(size.width, size.height))
            .map_err(|e| operation("set size", e))
    }
    fn restore_position(&self, saved: Option<Position>) -> OverlayResult<Position> {
        let window = self.window()?;
        let monitors = window
            .available_monitors()
            .map_err(|e| operation("list displays", e))?;
        let convert = |m: &tauri::Monitor| {
            let area = m.work_area();
            DisplayRect {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        };
        let primary = window
            .primary_monitor()
            .map_err(|e| operation("get primary display", e))?
            .or_else(|| monitors.first().cloned())
            .ok_or_else(|| operation("restore position", "no display is available"))?;
        let displays: Vec<_> = monitors.iter().map(convert).collect();
        let size = window
            .outer_size()
            .map_err(|e| operation("get physical size", e))?;
        let position = restore_position(
            saved,
            (size.width, size.height),
            &displays,
            convert(&primary),
        );
        self.set_position(position)?;
        Ok(position)
    }
    fn current_position(&self) -> OverlayResult<Position> {
        let position = self
            .window()?
            .outer_position()
            .map_err(|e| operation("get position", e))?;
        Ok(Position {
            x: position.x,
            y: position.y,
        })
    }
    fn start_dragging(&self) -> OverlayResult {
        self.window()?
            .start_dragging()
            .map_err(|e| operation("start dragging", e))
    }
}
