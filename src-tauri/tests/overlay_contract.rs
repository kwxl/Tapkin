//! Runs on Linux with no Tauri dependency or window. This is the renderer contract test.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tapkin_core::{
    config::AppSettings,
    core::{apply_overlay_settings, restore_overlay_settings, TapkinApp},
    input::InputEvent,
    overlay::{FrameRef, OverlayError, OverlayRenderer, OverlayResult},
    skin::{install_example, Skin},
    window::{Position, Size},
};

#[derive(Debug, PartialEq)]
enum Operation {
    Frame {
        index: usize,
        revision: u64,
        sequence: u64,
        png: Vec<u8>,
    },
    Visible(bool),
    Top(bool),
    Through(bool),
    Position(Position),
    Size(Size),
    Restore(Option<Position>),
    Drag,
}

struct TestOverlayRenderer {
    operations: Mutex<Vec<Operation>>,
    position: Mutex<Position>,
    fail_frame: AtomicBool,
}
impl Default for TestOverlayRenderer {
    fn default() -> Self {
        Self {
            operations: Mutex::new(Vec::new()),
            position: Mutex::new(Position { x: 20, y: 30 }),
            fail_frame: AtomicBool::new(false),
        }
    }
}
impl TestOverlayRenderer {
    fn record(&self, operation: Operation) {
        self.operations.lock().unwrap().push(operation);
    }
    fn frame_indexes(&self) -> Vec<usize> {
        self.operations
            .lock()
            .unwrap()
            .iter()
            .filter_map(|op| match op {
                Operation::Frame { index, .. } => Some(*index),
                _ => None,
            })
            .collect()
    }
}
impl OverlayRenderer for TestOverlayRenderer {
    fn show_frame(&self, frame: &FrameRef<'_>) -> OverlayResult {
        if self.fail_frame.swap(false, Ordering::AcqRel) {
            return Err(OverlayError::OperationFailed {
                operation: "show frame",
                message: "test renderer failure".into(),
            });
        }
        self.record(Operation::Frame {
            index: frame.index,
            revision: frame.revision,
            sequence: frame.sequence,
            png: frame.png.to_vec(),
        });
        Ok(())
    }
    fn set_visible(&self, visible: bool) -> OverlayResult {
        self.record(Operation::Visible(visible));
        Ok(())
    }
    fn set_always_on_top(&self, enabled: bool) -> OverlayResult {
        self.record(Operation::Top(enabled));
        Ok(())
    }
    fn set_click_through(&self, enabled: bool) -> OverlayResult {
        self.record(Operation::Through(enabled));
        Ok(())
    }
    fn set_position(&self, position: Position) -> OverlayResult {
        *self.position.lock().unwrap() = position;
        self.record(Operation::Position(position));
        Ok(())
    }
    fn set_size(&self, size: Size) -> OverlayResult {
        self.record(Operation::Size(size));
        Ok(())
    }
    fn restore_position(&self, saved: Option<Position>) -> OverlayResult<Position> {
        self.record(Operation::Restore(saved));
        let position = saved.unwrap_or(Position { x: 20, y: 30 });
        *self.position.lock().unwrap() = position;
        Ok(position)
    }
    fn current_position(&self) -> OverlayResult<Position> {
        Ok(*self.position.lock().unwrap())
    }
    fn start_dragging(&self) -> OverlayResult {
        self.record(Operation::Drag);
        Ok(())
    }
}

fn skin(hold: u64) -> (tempfile::TempDir, Skin) {
    let dir = tempfile::tempdir().unwrap();
    let root = install_example(dir.path()).unwrap();
    let mut skin = Skin::load(&root).unwrap();
    skin.config.frame_hold_ms = hold;
    (dir, skin)
}
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn non_tauri_core_selects_cached_frames_and_returns_to_idle() {
    let (dir, skin) = skin(0);
    let expected: Vec<_> = (0..3)
        .map(|index| skin.frame_png(index).unwrap().to_vec())
        .collect();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(skin, AppSettings::default(), None, overlay.clone());
    core.initialize_overlay().unwrap();
    core.overlay().set_visible(true).unwrap();
    // Editing the source file must not change what the renderer receives until reload.
    std::fs::write(dir.path().join("example/typing_1.png"), b"bad edit").unwrap();
    for now in [0, 10, 20] {
        core.on_input(InputEvent::AnyKeyPressed, ms(now)).unwrap();
    }
    assert_eq!(core.tick(ms(199)).unwrap(), None);
    assert_eq!(core.tick(ms(200)).unwrap().unwrap().frame, 0);
    assert_eq!(overlay.frame_indexes(), vec![0, 1, 2, 1, 0]);
    assert_eq!(core.next_deadline(), None);
    let ops = overlay.operations.lock().unwrap();
    let mut previous_sequence = 0;
    for op in ops.iter() {
        if let Operation::Frame {
            index,
            png,
            sequence,
            revision,
        } = op
        {
            assert_eq!(png, &expected[*index]);
            assert_eq!(*revision, 0);
            assert!(*sequence > previous_sequence);
            previous_sequence = *sequence;
        }
    }
}

#[test]
fn held_frame_and_reset_request_the_same_renderer() {
    let (_dir, skin) = skin(60);
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(skin, AppSettings::default(), None, overlay.clone());
    core.initialize_overlay().unwrap();
    core.on_input(InputEvent::AnyKeyPressed, ms(0)).unwrap();
    assert_eq!(
        core.on_input(InputEvent::AnyKeyPressed, ms(10)).unwrap(),
        None
    );
    assert_eq!(core.tick(ms(59)).unwrap(), None);
    core.tick(ms(60)).unwrap();
    core.reset_animation().unwrap();
    assert_eq!(overlay.frame_indexes(), vec![0, 1, 2, 0]);
    assert_eq!(core.next_deadline(), None);
}

#[test]
fn overlay_settings_and_rollback_use_only_the_trait() {
    let overlay = TestOverlayRenderer::default();
    let settings = AppSettings {
        always_on_top: false,
        click_through: true,
        window_size: Size {
            width: 300.0,
            height: 150.0,
        },
        window_position: Some(Position { x: -400, y: 200 }),
        ..Default::default()
    };
    assert_eq!(
        apply_overlay_settings(&overlay, &settings).unwrap(),
        Position { x: -400, y: 200 }
    );
    restore_overlay_settings(&overlay, &settings).unwrap();
    assert_eq!(
        *overlay.operations.lock().unwrap(),
        vec![
            Operation::Top(false),
            Operation::Through(true),
            Operation::Size(settings.window_size),
            Operation::Restore(settings.window_position),
            Operation::Top(false),
            Operation::Through(true),
            Operation::Size(settings.window_size),
            Operation::Position(settings.window_position.unwrap()),
        ]
    );
}

#[test]
fn reload_uses_a_new_revision_and_failed_render_keeps_old_core_state() {
    let (_dir, original) = skin(0);
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(original, AppSettings::default(), None, overlay.clone());
    core.initialize_overlay().unwrap();
    core.on_input(InputEvent::AnyKeyPressed, ms(0)).unwrap();
    let (_other, mut replacement) = skin(0);
    replacement.config.name = "Replacement".into();
    let settings = AppSettings {
        selected_skin: replacement.view.directory.clone(),
        ..Default::default()
    };
    let update = core.replace_skin(replacement, settings.clone()).unwrap();
    assert_eq!(update.revision, 1);
    assert_eq!(update.frame.revision, 1);
    assert!(update.frame.sequence > update.sequence);
    assert_eq!(core.frame(), 0);
    assert_eq!(core.settings(), &settings);
    assert_eq!(core.next_deadline(), None);
    let (_bad, mut failing) = skin(0);
    failing.config.name = "Must not replace".into();
    overlay.fail_frame.store(true, Ordering::Release);
    let error = core
        .replace_skin(failing, AppSettings::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("test renderer failure"));
    assert_eq!(core.skin().config.name, "Replacement");
    assert_eq!(core.settings(), &settings);
    core.on_input(InputEvent::AnyKeyPressed, ms(500)).unwrap();
    assert_eq!(core.frame(), 1);
}

#[test]
fn movement_deadline_and_shutdown_capture_are_renderer_neutral() {
    let (_dir, skin) = skin(0);
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(skin, AppSettings::default(), None, overlay.clone());
    core.record_position(Position { x: -30, y: 50 }, ms(100));
    assert_eq!(core.next_deadline(), Some(ms(350)));
    overlay.set_position(Position { x: -60, y: 100 }).unwrap();
    core.capture_position().unwrap();
    assert_eq!(
        core.settings().window_position,
        Some(Position { x: -60, y: 100 })
    );
}

#[test]
fn locked_or_click_through_core_disallows_dragging() {
    for (locked, through, allowed) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let (_dir, skin) = skin(0);
        let settings = AppSettings {
            lock_position: locked,
            click_through: through,
            ..Default::default()
        };
        let core = TapkinApp::new(
            skin,
            settings,
            None,
            Arc::new(TestOverlayRenderer::default()),
        );
        assert_eq!(core.drag_allowed(), allowed);
    }
}
