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
    input::{InputEvent, KeyCode},
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

fn skin() -> (tempfile::TempDir, Skin) {
    let dir = tempfile::tempdir().unwrap();
    let root = install_example(dir.path()).unwrap();
    let skin = Skin::load(&root).unwrap();
    (dir, skin)
}
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn down(id: u32, repeat: bool) -> InputEvent {
    InputEvent::KeyDown {
        id,
        key: KeyCode::parse("KeyA"),
        character: Some('a'),
        repeat,
    }
}

#[test]
fn hold_mode_ignores_repeats_and_times_out_only_after_last_release() {
    let (_dir, skin) = skin();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let settings = AppSettings {
        repeat_held_keys: false,
        frame_hold_ms: 0,
        ..Default::default()
    };
    let mut core = TapkinApp::new(skin, settings, None, overlay.clone());
    core.initialize_overlay().unwrap();
    assert_eq!(
        core.on_input(down(1, false), ms(0)).unwrap().unwrap().frame,
        1
    );
    assert_eq!(core.next_deadline(), None);
    assert_eq!(core.on_input(down(1, true), ms(500)).unwrap(), None);
    assert_eq!(core.on_input(down(1, false), ms(600)).unwrap(), None);
    assert_eq!(core.tick(ms(1000)).unwrap(), None);
    assert_eq!(
        core.on_input(down(2, false), ms(1100))
            .unwrap()
            .unwrap()
            .frame,
        2
    );
    core.on_input(InputEvent::KeyUp { id: 1 }, ms(1200))
        .unwrap();
    assert_eq!(core.next_deadline(), None);
    core.on_input(InputEvent::KeyUp { id: 99 }, ms(1250))
        .unwrap();
    assert_eq!(core.next_deadline(), None);
    core.on_input(InputEvent::KeyUp { id: 2 }, ms(1300))
        .unwrap();
    assert_eq!(core.next_deadline(), Some(ms(1480)));
    assert_eq!(core.tick(ms(1480)).unwrap().unwrap().frame, 0);
    core.on_input(down(1, false), ms(1500)).unwrap();
    assert_eq!(core.frame(), 1);
    core.on_input(InputEvent::ResetKeys, ms(1600)).unwrap();
    assert_eq!(core.frame(), 0);
    assert_eq!(core.next_deadline(), None);
}

#[test]
fn default_mode_counts_repeat_presses_and_release_does_not_extend_timeout() {
    let (_dir, skin) = skin();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(
        skin,
        AppSettings {
            frame_hold_ms: 0,
            ..Default::default()
        },
        None,
        overlay,
    );
    core.on_input(down(1, false), ms(0)).unwrap();
    assert_eq!(
        core.on_input(down(1, true), ms(100))
            .unwrap()
            .unwrap()
            .frame,
        2
    );
    core.on_input(InputEvent::KeyUp { id: 1 }, ms(150)).unwrap();
    assert_eq!(core.next_deadline(), Some(ms(280)));
    assert_eq!(core.tick(ms(280)).unwrap().unwrap().frame, 0);
}

#[test]
fn mapped_hold_keeps_character_frame_and_pending_frame_hold_swap() {
    let dir = tempfile::tempdir().unwrap();
    let root = install_example(dir.path()).unwrap();
    std::fs::write(root.join("pet.toml"), "name='Hold Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']\n[key_mappings]\n'?'='typing_2.png'\n'!'='typing_1.png'").unwrap();
    let settings = AppSettings {
        repeat_held_keys: false,
        ..Default::default()
    };
    let mut core = TapkinApp::new(
        Skin::load(&root).unwrap(),
        settings,
        None,
        Arc::new(TestOverlayRenderer::default()),
    );
    let press = |id, character| InputEvent::KeyDown {
        id,
        key: None,
        character: Some(character),
        repeat: false,
    };
    assert_eq!(
        core.on_input(press(1, '?'), ms(0)).unwrap().unwrap().frame,
        2
    );
    assert_eq!(core.on_input(press(2, '!'), ms(10)).unwrap(), None);
    assert_eq!(core.tick(ms(60)).unwrap().unwrap().frame, 1);
    assert_eq!(core.on_input(press(1, '?'), ms(500)).unwrap(), None);
    assert_eq!(core.tick(ms(1000)).unwrap(), None);
    assert_eq!(core.frame(), 1);
    core.on_input(InputEvent::KeyUp { id: 1 }, ms(1100))
        .unwrap();
    core.on_input(InputEvent::KeyUp { id: 2 }, ms(1200))
        .unwrap();
    assert_eq!(core.tick(ms(1380)).unwrap().unwrap().frame, 0);
}

#[test]
fn mapped_keys_select_cached_images_and_reload_removes_mappings() {
    let dir = tempfile::tempdir().unwrap();
    let root = install_example(dir.path()).unwrap();
    std::fs::copy(root.join("typing_1.png"), root.join("special.png")).unwrap();
    std::fs::write(root.join("pet.toml"), "name='Mapped Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']\nframe_hold_ms=0\n[key_mappings]\nKeyA='special.png'").unwrap();
    let mapped = Skin::load(&root).unwrap();
    let expected = mapped.frame_png(3).unwrap().to_vec();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(mapped, AppSettings::default(), None, overlay.clone());
    core.initialize_overlay().unwrap();
    let key = InputEvent::KeyPressed(KeyCode::parse("KeyA").unwrap());
    core.on_input(key, ms(0)).unwrap();
    assert_eq!(core.frame(), 3);
    std::fs::write(root.join("special.png"), b"bad edit").unwrap();
    assert_eq!(core.on_input(key, ms(100)).unwrap(), None);
    assert_eq!(core.tick(ms(180)).unwrap(), None);
    assert_eq!(core.tick(ms(280)).unwrap().unwrap().frame, 0);
    core.on_input(
        InputEvent::KeyPressed(KeyCode::parse("KeyB").unwrap()),
        ms(300),
    )
    .unwrap();
    assert_eq!(core.frame(), 1);
    let (_other, replacement) = skin();
    core.replace_skin(replacement, AppSettings::default())
        .unwrap();
    core.on_input(key, ms(400)).unwrap();
    assert_eq!(core.frame(), 1);
    assert_eq!(overlay.frame_indexes(), vec![0, 3, 0, 1, 0, 1]);
    assert!(overlay
        .operations
        .lock()
        .unwrap()
        .iter()
        .any(|op| matches!(op, Operation::Frame { index: 3, png, .. } if *png == expected)));
}

#[test]
fn characters_override_physical_mappings_and_retain_hold_and_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let root = install_example(dir.path()).unwrap();
    std::fs::copy(root.join("typing_1.png"), root.join("question.png")).unwrap();
    std::fs::write(root.join("pet.toml"), "name='Characters'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']\n[key_mappings]\n'?'='question.png'\n'!'='typing_1.png'\nSlash='typing_2.png'").unwrap();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(
        Skin::load(&root).unwrap(),
        AppSettings::default(),
        None,
        overlay.clone(),
    );
    let press = |character| InputEvent::CharacterPressed {
        key: KeyCode::parse("Slash"),
        character,
    };
    core.initialize_overlay().unwrap();
    assert_eq!(core.on_input(press('?'), ms(0)).unwrap().unwrap().frame, 3);
    assert_eq!(core.on_input(press('!'), ms(10)).unwrap(), None);
    assert_eq!(core.tick(ms(60)).unwrap().unwrap().frame, 1);
    assert_eq!(
        core.on_input(press('/'), ms(120)).unwrap().unwrap().frame,
        2
    );
    assert_eq!(
        core.on_input(press('?'), ms(180)).unwrap().unwrap().frame,
        3
    );
    assert_eq!(core.on_input(press('?'), ms(280)).unwrap(), None);
    assert_eq!(core.tick(ms(360)).unwrap(), None);
    assert_eq!(core.tick(ms(460)).unwrap().unwrap().frame, 0);
    core.on_input(
        InputEvent::CharacterPressed {
            key: None,
            character: '?',
        },
        ms(500),
    )
    .unwrap();
    assert_eq!(core.frame(), 3);
    let (_other, replacement) = skin();
    core.replace_skin(replacement, AppSettings::default())
        .unwrap();
    core.on_input(press('?'), ms(600)).unwrap();
    assert_eq!(core.frame(), 1);
}

#[test]
fn non_tauri_core_selects_cached_frames_and_returns_to_idle() {
    let (dir, skin) = skin();
    let expected: Vec<_> = (0..3)
        .map(|index| skin.frame_png(index).unwrap().to_vec())
        .collect();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(
        skin,
        AppSettings {
            frame_hold_ms: 0,
            ..Default::default()
        },
        None,
        overlay.clone(),
    );
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
    let (_dir, skin) = skin();
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
fn client_timing_controls_animation_and_survives_skin_replacement() {
    let (_dir, original) = skin();
    let settings = AppSettings {
        typing_timeout_ms: 500,
        frame_hold_ms: 100,
        ..Default::default()
    };
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(original, settings.clone(), None, overlay);
    core.initialize_overlay().unwrap();
    core.on_input(InputEvent::AnyKeyPressed, ms(0)).unwrap();
    assert_eq!(
        core.on_input(InputEvent::AnyKeyPressed, ms(10)).unwrap(),
        None
    );
    assert_eq!(core.tick(ms(60)).unwrap(), None);
    assert_eq!(core.tick(ms(100)).unwrap().unwrap().frame, 2);
    assert_eq!(core.tick(ms(190)).unwrap(), None);
    assert_eq!(core.tick(ms(510)).unwrap().unwrap().frame, 0);
    let (_other, replacement) = skin();
    core.replace_skin(replacement, settings).unwrap();
    core.on_input(InputEvent::AnyKeyPressed, ms(600)).unwrap();
    assert_eq!(core.tick(ms(780)).unwrap(), None);
    assert_eq!(core.tick(ms(1100)).unwrap().unwrap().frame, 0);
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
    let (_dir, original) = skin();
    let overlay = Arc::new(TestOverlayRenderer::default());
    let mut core = TapkinApp::new(original, AppSettings::default(), None, overlay.clone());
    core.initialize_overlay().unwrap();
    core.on_input(InputEvent::AnyKeyPressed, ms(0)).unwrap();
    let (_other, mut replacement) = skin();
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
    let (_bad, mut failing) = skin();
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
    let (_dir, skin) = skin();
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
        let (_dir, skin) = skin();
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
