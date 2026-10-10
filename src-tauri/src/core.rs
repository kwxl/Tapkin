//! Renderer-independent pet state and orchestration. No Tauri window or event APIs.
use crate::{
    config::AppSettings,
    input::InputEvent,
    overlay::{FrameRef, OverlayError, OverlayRenderer, OverlayResult},
    skin::Skin,
    state::Animation,
    window::Position,
};
use serde::Serialize;
use std::{collections::BTreeSet, sync::Arc, time::Duration};

#[derive(Clone, Serialize)]
pub struct InputStatus {
    pub active: bool,
    pub retrying: bool,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FrameChange {
    pub revision: u64,
    pub frame: usize,
    pub sequence: u64,
}

pub struct SkinChange {
    pub revision: u64,
    pub sequence: u64,
    pub frame: FrameChange,
}

pub struct TapkinApp {
    pub(crate) skin: Skin,
    pub(crate) settings: AppSettings,
    animation: Animation,
    pub(crate) frame: usize,
    pub(crate) revision: u64,
    pub(crate) sequence: u64,
    pub(crate) input: InputStatus,
    pub(crate) warning: Option<String>,
    pub(crate) save_at: Option<Duration>,
    overlay: Arc<dyn OverlayRenderer>,
    held_keys: BTreeSet<u32>,
}

impl TapkinApp {
    pub fn new(
        skin: Skin,
        settings: AppSettings,
        warning: Option<String>,
        overlay: Arc<dyn OverlayRenderer>,
    ) -> Self {
        let animation = Animation::new(
            skin.config.typing.len(),
            settings.typing_timeout_ms,
            settings.frame_hold_ms,
        );
        Self {
            skin,
            settings,
            animation,
            frame: 0,
            revision: 0,
            sequence: 0,
            input: InputStatus {
                active: false,
                retrying: true,
                message: None,
            },
            warning,
            save_at: None,
            overlay,
            held_keys: BTreeSet::new(),
        }
    }

    pub fn overlay(&self) -> Arc<dyn OverlayRenderer> {
        Arc::clone(&self.overlay)
    }
    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }
    pub fn apply_animation_timing(&mut self) {
        self.animation
            .set_timing(self.settings.typing_timeout_ms, self.settings.frame_hold_ms);
    }
    pub fn input_status(&self) -> &InputStatus {
        &self.input
    }
    pub fn skin(&self) -> &Skin {
        &self.skin
    }
    pub fn frame(&self) -> usize {
        self.frame
    }

    pub fn initialize_overlay(&mut self) -> OverlayResult<FrameChange> {
        self.settings.window_position = Some(apply_overlay_settings(
            self.overlay.as_ref(),
            &self.settings,
        )?);
        self.present(0)
    }

    pub fn on_input(
        &mut self,
        event: InputEvent,
        now: Duration,
    ) -> OverlayResult<Option<FrameChange>> {
        let event = match event {
            InputEvent::ResetKeys => {
                return self.reset_animation().map(Some);
            }
            InputEvent::KeyUp { id } => {
                self.held_keys.remove(&id);
                self.animation.set_held(
                    !self.settings.repeat_held_keys && !self.held_keys.is_empty(),
                    now,
                );
                return Ok(None);
            }
            InputEvent::KeyDown {
                id,
                key,
                character,
                repeat,
            } => {
                let first_press = self.held_keys.insert(id);
                self.animation.set_held(
                    !self.settings.repeat_held_keys && !self.held_keys.is_empty(),
                    now,
                );
                if !self.settings.repeat_held_keys && (!first_press || repeat) {
                    return Ok(None);
                }
                match character {
                    Some(character) => InputEvent::CharacterPressed { key, character },
                    None => key.map_or(InputEvent::AnyKeyPressed, InputEvent::KeyPressed),
                }
            }
            event => event,
        };
        let mapped = match event {
            InputEvent::AnyKeyPressed => None,
            InputEvent::KeyPressed(key) => self.skin.mapped_frame(key),
            InputEvent::CharacterPressed { key, character } => self
                .skin
                .mapped_character(character)
                .or_else(|| key.and_then(|key| self.skin.mapped_frame(key))),
            InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. } | InputEvent::ResetKeys => {
                unreachable!()
            }
        };
        let frame = match mapped {
            Some(index) => self.animation.key_frame(now, Some(index)),
            None => self.animation.key(now),
        };
        frame.map(|index| self.present(index)).transpose()
    }

    pub fn tick(&mut self, now: Duration) -> OverlayResult<Option<FrameChange>> {
        self.animation.set_held(
            !self.settings.repeat_held_keys && !self.held_keys.is_empty(),
            now,
        );
        self.animation
            .tick(now)
            .map(|index| self.present(index))
            .transpose()
    }

    pub fn reset_animation(&mut self) -> OverlayResult<FrameChange> {
        self.held_keys.clear();
        self.animation.reset();
        self.present(0)
    }

    fn present(&mut self, index: usize) -> OverlayResult<FrameChange> {
        let change = FrameChange {
            revision: self.revision,
            frame: index,
            sequence: self.sequence + 1,
        };
        self.overlay.show_frame(&frame_ref(&self.skin, change)?)?;
        self.frame = index;
        self.sequence = change.sequence;
        Ok(change)
    }

    /// Present the validated replacement before committing state. A failed renderer keeps
    /// the old skin, settings and animation available for the shell's rollback/retry.
    pub fn replace_skin(&mut self, skin: Skin, settings: AppSettings) -> OverlayResult<SkinChange> {
        let revision = self.revision + 1;
        let sequence = self.sequence + 1;
        let frame = FrameChange {
            revision,
            frame: 0,
            sequence: sequence + 1,
        };
        self.overlay.show_frame(&frame_ref(&skin, frame)?)?;
        self.animation = Animation::new(
            skin.config.typing.len(),
            settings.typing_timeout_ms,
            settings.frame_hold_ms,
        );
        self.held_keys.clear();
        self.skin = skin;
        self.settings = settings;
        self.revision = revision;
        self.sequence = frame.sequence;
        self.frame = 0;
        self.save_at = None;
        self.warning = None;
        Ok(SkinChange {
            revision,
            sequence,
            frame,
        })
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        match (self.animation.next_deadline(), self.save_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn record_position(&mut self, position: Position, now: Duration) {
        self.settings.window_position = Some(position);
        self.save_at = Some(now + Duration::from_millis(250));
    }

    /// Read the renderer at shutdown so a final queued move cannot lose its position.
    pub fn capture_position(&mut self) -> OverlayResult {
        self.settings.window_position = Some(self.overlay.current_position()?);
        Ok(())
    }

    pub fn drag_allowed(&self) -> bool {
        !self.settings.lock_position && !self.settings.click_through
    }
}

fn frame_ref(skin: &Skin, change: FrameChange) -> OverlayResult<FrameRef<'_>> {
    Ok(FrameRef {
        png: skin
            .frame_png(change.frame)
            .ok_or(OverlayError::InvalidFrame(change.frame))?,
        name: &skin.config.name,
        index: change.frame,
        revision: change.revision,
        sequence: change.sequence,
    })
}

/// Execute geometry outside the shared core-state lock: renderer implementations may
/// need to dispatch to their UI thread. Persistence and login integration stay in the shell.
pub fn apply_overlay_settings(
    overlay: &dyn OverlayRenderer,
    settings: &AppSettings,
) -> OverlayResult<Position> {
    apply_overlay_properties(overlay, settings)?;
    overlay.restore_position(settings.window_position)
}

pub fn restore_overlay_settings(
    overlay: &dyn OverlayRenderer,
    settings: &AppSettings,
) -> OverlayResult {
    apply_overlay_properties(overlay, settings)?;
    if let Some(position) = settings.window_position {
        overlay.set_position(position)?;
    }
    Ok(())
}

fn apply_overlay_properties(
    overlay: &dyn OverlayRenderer,
    settings: &AppSettings,
) -> OverlayResult {
    overlay.set_always_on_top(settings.always_on_top)?;
    overlay.set_click_through(settings.click_through)?;
    overlay.set_size(settings.window_size)
}
