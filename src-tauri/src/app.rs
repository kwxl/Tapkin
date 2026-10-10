use crate::{
    config::AppSettings,
    core::{apply_overlay_settings, restore_overlay_settings, FrameChange, InputStatus, TapkinApp},
    input::{platform_backend, InputEvent, InputListener},
    overlay::{tauri::TauriOverlayRenderer, OverlayRenderer},
    skin::{install_example, Skin, SkinView},
    window::{aspect_size, Position},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::Instant,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

struct Runtime {
    data: Arc<Mutex<TapkinApp>>,
    tx: SyncSender<EngineEvent>,
    settings_path: PathBuf,
    listener: Mutex<Option<Box<dyn InputListener>>>,
    generation: AtomicU64,
    mutations: Mutex<()>,
    input_overflow: Arc<AtomicBool>,
}
enum EngineEvent {
    Key(InputEvent),
    Wake,
    Moved(Position),
    Failed(u64, String),
    Quit,
}

#[derive(Serialize)]
struct View {
    skin: SkinView,
    settings: AppSettings,
    frame: usize,
    revision: u64,
    sequence: u64,
    input: InputStatus,
    warning: Option<String>,
    platform: &'static str,
}
#[derive(Clone, Serialize)]
struct SkinEvent {
    revision: u64,
    skin: SkinView,
    sequence: u64,
}
#[derive(Clone, Serialize)]
struct SettingsEvent {
    settings: AppSettings,
    sequence: u64,
}
#[derive(Clone, Serialize)]
struct InputStatusEvent {
    input: InputStatus,
    sequence: u64,
}
#[derive(Clone, Serialize)]
struct WarningEvent {
    message: String,
    sequence: u64,
}
struct TrayItems {
    skin_name: MenuItem<tauri::Wry>,
    top: CheckMenuItem<tauri::Wry>,
    through: CheckMenuItem<tauri::Wry>,
    locked: CheckMenuItem<tauri::Wry>,
    login: CheckMenuItem<tauri::Wry>,
}

fn show_settings(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("settings") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}
fn report_error(app: &AppHandle, message: String) {
    {
        let runtime = app.state::<Runtime>();
        let mut data = runtime.data.lock().unwrap();
        data.warning = Some(message.clone());
        data.sequence += 1;
        let _ = app.emit_to(
            "settings",
            "app-warning",
            WarningEvent {
                message,
                sequence: data.sequence,
            },
        );
    }
    show_settings(app);
}
fn emit_preview(app: &AppHandle, frame: FrameChange) {
    // Settings preview remains usable even when the pet uses a non-Tauri renderer.
    let _ = app.emit_to("settings", "pet-frame", frame);
}

fn emit_input_status(app: &AppHandle, data: &mut TapkinApp) {
    data.sequence += 1;
    let _ = app.emit_to(
        "settings",
        "input-status",
        InputStatusEvent {
            input: data.input.clone(),
            sequence: data.sequence,
        },
    );
}

fn emit_settings(app: &AppHandle, data: &mut TapkinApp) {
    data.sequence += 1;
    let _ = app.emit_to(
        "settings",
        "settings-changed",
        SettingsEvent {
            settings: data.settings.clone(),
            sequence: data.sequence,
        },
    );
}

fn engine(
    app: AppHandle,
    data: Arc<Mutex<TapkinApp>>,
    rx: Receiver<EngineEvent>,
    origin: Instant,
    settings_path: PathBuf,
) {
    loop {
        let deadline = data.lock().unwrap().next_deadline();
        // Block indefinitely at idle; only animation or a pending settings write needs a timer.
        let event = match deadline {
            Some(at) => match rx.recv_timeout(at.saturating_sub(origin.elapsed())) {
                Ok(event) => Some(event),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(event) => Some(event),
                Err(_) => break,
            },
        };
        let mut failed = false;
        let mut save_error = None;
        let mut overlay_error = None;
        {
            let mut data = data.lock().unwrap();
            let now = origin.elapsed();
            if app
                .state::<Runtime>()
                .input_overflow
                .swap(false, Ordering::AcqRel)
            {
                match data.reset_animation() {
                    Ok(frame) => emit_preview(&app, frame),
                    Err(error) => overlay_error = Some(error.to_string()),
                }
            }
            match event {
                Some(EngineEvent::Quit) => break,
                Some(EngineEvent::Key(event)) if data.input.active || cfg!(debug_assertions) => {
                    match data.on_input(event, now) {
                        Ok(Some(frame)) => emit_preview(&app, frame),
                        Ok(None) => {}
                        Err(error) => overlay_error = Some(error.to_string()),
                    }
                }
                Some(EngineEvent::Failed(generation, message))
                    if generation == app.state::<Runtime>().generation.load(Ordering::Acquire) =>
                {
                    data.input = InputStatus {
                        active: false,
                        retrying: false,
                        message: Some(message),
                    };
                    match data.reset_animation() {
                        Ok(frame) => emit_preview(&app, frame),
                        Err(error) => overlay_error = Some(error.to_string()),
                    }
                    emit_input_status(&app, &mut data);
                    failed = true;
                }
                Some(EngineEvent::Moved(position)) => data.record_position(position, now),
                _ => {}
            }
            match data.tick(now) {
                Ok(Some(frame)) => emit_preview(&app, frame),
                Ok(None) => {}
                Err(error) => overlay_error = Some(error.to_string()),
            }
            if data.save_at.is_some_and(|at| now >= at) {
                data.save_at = None;
                if let Err(e) = data.settings.save(&settings_path) {
                    save_error = Some(format!("Could not save window position: {e}"));
                }
            }
        }
        if failed {
            show_settings(&app);
        }
        if let Some(error) = save_error {
            report_error(&app, error);
        }
        if let Some(error) = overlay_error {
            report_error(&app, error);
        }
    }
}

fn start_listener(app: &AppHandle) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let mut listener = runtime.listener.lock().unwrap();
    let generation = runtime.generation.fetch_add(1, Ordering::AcqRel) + 1;
    listener.take(); // Stop the previous hook; queued old failures are ignored.
    let render_error = {
        let mut data = runtime.data.lock().unwrap();
        data.input = InputStatus {
            active: false,
            retrying: true,
            message: None,
        };
        let error = match data.reset_animation() {
            Ok(frame) => {
                emit_preview(app, frame);
                None
            }
            Err(error) => Some(error.to_string()),
        };
        emit_input_status(app, &mut data);
        error
    };
    if let Some(error) = render_error {
        report_error(app, error);
    }
    let activity_tx = runtime.tx.clone();
    let failure_tx = runtime.tx.clone();
    let input_overflow = Arc::clone(&runtime.input_overflow);
    let result = platform_backend().start(
        Arc::new(move |event| {
            // Never wait in an OS keyboard callback. Bound the queue under extreme input load.
            if activity_tx.try_send(EngineEvent::Key(event)).is_err() {
                // A dropped release must never leave the pet stuck in hold mode.
                input_overflow.store(true, Ordering::Release);
            }
        }),
        Arc::new(move |message| {
            let _ = failure_tx.send(EngineEvent::Failed(generation, message));
        }),
    );
    let error = match result {
        Ok(handle) => {
            *listener = Some(handle);
            None
        }
        Err(error) => Some(error),
    };
    {
        let mut data = runtime.data.lock().unwrap();
        if data.input.retrying {
            data.input = InputStatus {
                active: error.is_none(),
                retrying: false,
                message: error.clone(),
            };
        }
        emit_input_status(app, &mut data);
    }
    if let Some(error) = error {
        show_settings(app);
        Err(error)
    } else {
        Ok(())
    }
}

#[tauri::command]
fn get_view(runtime: tauri::State<'_, Runtime>) -> View {
    let data = runtime.data.lock().unwrap();
    View {
        skin: data.skin.view.clone(),
        settings: data.settings.clone(),
        frame: data.frame,
        revision: data.revision,
        sequence: data.sequence,
        input: data.input.clone(),
        warning: data.warning.clone(),
        platform: std::env::consts::OS,
    }
}
fn sync_tray(app: &AppHandle) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let (name, settings) = {
        let data = runtime.data.lock().unwrap();
        (data.skin.config.name.clone(), data.settings.clone())
    };
    // Native menu setters dispatch to the main thread. Never hold core state while waiting.
    let items = app.state::<TrayItems>();
    items.skin_name.set_text(&name).map_err(|e| e.to_string())?;
    for (item, checked) in [
        (&items.top, settings.always_on_top),
        (&items.through, settings.click_through),
        (&items.locked, settings.lock_position),
        (&items.login, settings.launch_at_login),
    ] {
        item.set_checked(checked).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[derive(Default, Deserialize)]
struct SettingsPatch {
    repeat_held_keys: Option<bool>,
    typing_timeout_ms: Option<u64>,
    frame_hold_ms: Option<u64>,
    width: Option<f64>,
    always_on_top: Option<bool>,
    click_through: Option<bool>,
    lock_position: Option<bool>,
    launch_at_login: Option<bool>,
}
fn change_settings(app: &AppHandle, patch: SettingsPatch) -> Result<AppSettings, String> {
    let runtime = app.state::<Runtime>();
    let _mutation = runtime.mutations.lock().unwrap();
    let (old, canvas, overlay) = {
        let data = runtime.data.lock().unwrap();
        (
            data.settings.clone(),
            (data.skin.view.width, data.skin.view.height),
            data.overlay(),
        )
    };
    let mut settings = old.clone();
    if let Some(value) = patch.repeat_held_keys {
        settings.repeat_held_keys = value;
    }
    if let Some(value) = patch.typing_timeout_ms {
        settings.typing_timeout_ms = value;
    }
    if let Some(value) = patch.frame_hold_ms {
        settings.frame_hold_ms = value;
    }
    if !settings.valid_animation_timing() {
        return Err("Typing timeout must be 50–10000 ms; frame hold must be 0–timeout ms".into());
    }
    if let Some(width) = patch.width {
        if !width.is_finite() || !(64.0..=800.0).contains(&width) {
            return Err("Width must be 64–800 pixels".into());
        }
        settings.window_size = aspect_size(width, canvas);
    }
    if let Some(v) = patch.always_on_top {
        settings.always_on_top = v;
    }
    if let Some(v) = patch.click_through {
        settings.click_through = v;
    }
    if let Some(v) = patch.lock_position {
        settings.lock_position = v;
    }
    if let Some(v) = patch.launch_at_login {
        settings.launch_at_login = v;
    }
    let result = (|| {
        settings.window_position =
            Some(apply_overlay_settings(overlay.as_ref(), &settings).map_err(|e| e.to_string())?);
        if settings.launch_at_login != old.launch_at_login {
            let manager = app.autolaunch();
            if settings.launch_at_login {
                manager.enable()
            } else {
                manager.disable()
            }
            .map_err(|e| e.to_string())?;
        }
        let mut data = runtime.data.lock().unwrap();
        settings
            .save(&runtime.settings_path)
            .map_err(|e| format!("Could not save settings: {e}"))?;
        data.settings = settings.clone();
        data.apply_animation_timing();
        data.save_at = None;
        emit_settings(app, &mut data);
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let _ = restore_overlay_settings(overlay.as_ref(), &old);
        if settings.launch_at_login != old.launch_at_login {
            let manager = app.autolaunch();
            let _ = if old.launch_at_login {
                manager.enable()
            } else {
                manager.disable()
            };
        }
        let _ = sync_tray(app);
        return Err(error);
    }
    let _ = runtime.tx.try_send(EngineEvent::Wake);
    sync_tray(app)?;
    Ok(settings)
}
#[tauri::command]
async fn preview_size(app: AppHandle, width: f64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !width.is_finite() || !(64.0..=800.0).contains(&width) {
            return Err("Width must be 64–800 pixels".into());
        }
        let runtime = app.state::<Runtime>();
        let _mutation = runtime.mutations.lock().unwrap();
        let (canvas, overlay) = {
            let data = runtime.data.lock().unwrap();
            (
                (data.skin.view.width, data.skin.view.height),
                data.overlay(),
            )
        };
        overlay
            .set_size(aspect_size(width, canvas))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn update_settings(app: AppHandle, patch: SettingsPatch) -> Result<AppSettings, String> {
    tauri::async_runtime::spawn_blocking(move || change_settings(&app, patch))
        .await
        .map_err(|e| e.to_string())?
}
fn change_skin(app: &AppHandle, directory: PathBuf) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let _mutation = runtime.mutations.lock().unwrap();
    let skin = Skin::load(&directory).map_err(|e| e.to_string())?;
    let (old, overlay) = {
        let data = runtime.data.lock().unwrap();
        (data.settings.clone(), data.overlay())
    };
    let mut settings = old.clone();
    settings.selected_skin = skin.view.directory.clone();
    settings.window_size = aspect_size(
        settings.window_size.width,
        (skin.view.width, skin.view.height),
    );
    let mut saved = false;
    let result = (|| {
        settings.window_position =
            Some(apply_overlay_settings(overlay.as_ref(), &settings).map_err(|e| e.to_string())?);
        let mut data = runtime.data.lock().unwrap();
        settings
            .save(&runtime.settings_path)
            .map_err(|e| format!("Could not save skin selection: {e}"))?;
        saved = true;
        let update = data
            .replace_skin(skin, settings)
            .map_err(|e| e.to_string())?;
        let _ = app.emit_to(
            "settings",
            "skin-changed",
            SkinEvent {
                revision: update.revision,
                skin: data.skin.view.clone(),
                sequence: update.sequence,
            },
        );
        emit_preview(app, update.frame);
        emit_settings(app, &mut data);
        Ok::<(), String>(())
    })();
    if let Err(mut error) = result {
        let _ = restore_overlay_settings(overlay.as_ref(), &old);
        if saved {
            if let Err(save_error) = old.save(&runtime.settings_path) {
                error.push_str(&format!(
                    "; could not restore previous settings: {save_error}"
                ));
            }
        }
        return Err(error);
    }
    let _ = runtime.tx.try_send(EngineEvent::Wake);
    sync_tray(app)
}
#[tauri::command]
async fn load_skin(app: AppHandle, directory: PathBuf) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || change_skin(&app, directory))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn reload_skin(app: AppHandle) -> Result<(), String> {
    let directory = app
        .state::<Runtime>()
        .data
        .lock()
        .unwrap()
        .settings
        .selected_skin
        .clone();
    load_skin(app, directory).await
}
#[tauri::command]
fn open_skin_folder(app: AppHandle) -> Result<(), String> {
    let directory = app
        .state::<Runtime>()
        .data
        .lock()
        .unwrap()
        .settings
        .selected_skin
        .clone();
    app.opener()
        .open_path(directory.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn retry_listener(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start_listener(&app))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_privacy_settings(app: AppHandle, section: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let anchor = match section.as_str() {
            "input" => "Privacy_ListenEvent",
            "accessibility" => "Privacy_Accessibility",
            _ => return Err("Unknown privacy section".into()),
        };
        app.opener()
            .open_url(
                format!("x-apple.systempreferences:com.apple.preference.security?{anchor}"),
                None::<&str>,
            )
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (app, section);
        Err("Privacy settings shortcuts are available on macOS".into())
    }
}
#[tauri::command]
fn drag_pet(app: AppHandle) -> Result<(), String> {
    let runtime = app.state::<Runtime>();
    let (allowed, overlay) = {
        let data = runtime.data.lock().unwrap();
        (data.drag_allowed(), data.overlay())
    };
    if !allowed {
        return Ok(());
    }
    overlay.start_dragging().map_err(|e| e.to_string())
}
#[tauri::command]
fn show_settings_window(app: AppHandle) {
    show_settings(&app);
}
#[tauri::command]
fn local_test_input(app: AppHandle) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("Local test input is disabled in release builds".into());
    }
    app.state::<Runtime>()
        .tx
        .try_send(EngineEvent::Key(InputEvent::AnyKeyPressed))
        .map_err(|e| e.to_string())
}
fn flush_settings(runtime: &Runtime) -> Result<(), String> {
    let overlay = runtime.data.lock().unwrap().overlay();
    // Geometry calls run outside the state lock and include the last queued movement.
    let position = overlay.current_position().map_err(|e| e.to_string());
    let mut data = runtime.data.lock().unwrap();
    if let Ok(position) = &position {
        data.settings.window_position = Some(*position);
    }
    data.settings
        .save(&runtime.settings_path)
        .map_err(|e| e.to_string())?;
    position.map(|_| ())
}
fn quit(app: &AppHandle) {
    let runtime = app.state::<Runtime>();
    runtime.listener.lock().unwrap().take();
    if let Err(error) = flush_settings(&runtime) {
        report_error(
            app,
            format!("Could not save settings before quitting: {error}"),
        );
    }
    let overlay = runtime.data.lock().unwrap().overlay();
    let _ = overlay.set_visible(false);
    let _ = runtime.tx.send(EngineEvent::Quit);
    app.exit(0);
}
#[tauri::command]
fn quit_app(app: AppHandle) {
    quit(&app);
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = app.state::<Runtime>().data.lock().unwrap().settings.clone();
    let name = app
        .state::<Runtime>()
        .data
        .lock()
        .unwrap()
        .skin
        .config
        .name
        .clone();
    let label = MenuItem::new(app, "Tapkin", false, None::<&str>)?;
    let skin_name = MenuItem::new(app, name, false, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Skin", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Skin Folder", true, None::<&str>)?;
    let choose = MenuItem::with_id(app, "settings", "Choose Skin…", true, None::<&str>)?;
    let skin = Submenu::with_items(app, "Skin", true, &[&skin_name, &choose, &reload, &open])?;
    let top = CheckMenuItem::with_id(
        app,
        "top",
        "Always on Top",
        true,
        settings.always_on_top,
        None::<&str>,
    )?;
    let through = CheckMenuItem::with_id(
        app,
        "through",
        "Click Through",
        true,
        settings.click_through,
        None::<&str>,
    )?;
    let locked = CheckMenuItem::with_id(
        app,
        "locked",
        "Lock Position",
        true,
        settings.lock_position,
        None::<&str>,
    )?;
    let login = CheckMenuItem::with_id(
        app,
        "login",
        "Launch at Login",
        true,
        settings.launch_at_login,
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &label,
            &PredefinedMenuItem::separator(app)?,
            &skin,
            &PredefinedMenuItem::separator(app)?,
            &top,
            &through,
            &locked,
            &login,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &quit_item,
        ],
    )?;
    app.manage(TrayItems {
        skin_name,
        top,
        through,
        locked,
        login,
    });
    TrayIconBuilder::with_id("tapkin")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("Tapkin — desktop typing companion")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .build(app)?;
    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            app,
            "Tapkin",
            true,
            &[
                &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
                &MenuItem::with_id(app, "quit", "Quit Tapkin", true, Some("CmdOrCtrl+Q"))?,
            ],
        )?;
        app.set_menu(Menu::with_items(app, &[&app_menu])?)?;
    }
    app.on_menu_event(|app, event| {
        let app = app.clone();
        let id = event.id().as_ref().to_owned();
        if id == "quit" {
            quit(&app);
            return;
        }
        if id == "settings" {
            show_settings(&app);
            return;
        }
        tauri::async_runtime::spawn_blocking(move || {
            let result = match id.as_str() {
                "reload" => {
                    let dir = app
                        .state::<Runtime>()
                        .data
                        .lock()
                        .unwrap()
                        .settings
                        .selected_skin
                        .clone();
                    change_skin(&app, dir)
                }
                "open" => open_skin_folder(app.clone()),
                _ => {
                    let old = app.state::<Runtime>().data.lock().unwrap().settings.clone();
                    let patch = match id.as_str() {
                        "top" => SettingsPatch {
                            always_on_top: Some(!old.always_on_top),
                            ..Default::default()
                        },
                        "through" => SettingsPatch {
                            click_through: Some(!old.click_through),
                            ..Default::default()
                        },
                        "locked" => SettingsPatch {
                            lock_position: Some(!old.lock_position),
                            ..Default::default()
                        },
                        "login" => SettingsPatch {
                            launch_at_login: Some(!old.launch_at_login),
                            ..Default::default()
                        },
                        _ => return,
                    };
                    change_settings(&app, patch).map(|_| ())
                }
            };
            if let Err(error) = result {
                let _ = sync_tray(&app);
                report_error(&app, error);
            }
        });
    });
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_view,
            update_settings,
            preview_size,
            load_skin,
            reload_skin,
            open_skin_folder,
            retry_listener,
            open_privacy_settings,
            drag_pet,
            show_settings_window,
            local_test_input,
            quit_app
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let data_dir = app.path().app_data_dir()?;
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let example = install_example(&data_dir.join("skins"))?;
            let (mut settings, mut warning) = AppSettings::load(&settings_path);
            let selected = if settings.selected_skin.as_os_str().is_empty() {
                &example
            } else {
                &settings.selected_skin
            };
            let skin = match Skin::load(selected) {
                Ok(skin) => skin,
                Err(error) => {
                    warning = Some(format!(
                        "Could not load the selected skin: {error}. Using the bundled example."
                    ));
                    match Skin::load(&example) {
                        Ok(skin) => skin,
                        Err(_) => Skin::load(&install_example(&data_dir.join("recovery-skins"))?)?,
                    }
                }
            };
            settings.selected_skin = skin.view.directory.clone();
            settings.window_size = aspect_size(
                settings.window_size.width,
                (skin.view.width, skin.view.height),
            );
            match app.autolaunch().is_enabled() {
                Ok(enabled) => settings.launch_at_login = enabled,
                Err(error) => {
                    settings.launch_at_login = false;
                    warning = Some(format!("Could not check launch-at-login: {error}"));
                }
            }
            let renderer = Arc::new(TauriOverlayRenderer::new(app.handle().clone()));
            let data = Arc::new(Mutex::new(TapkinApp::new(
                skin,
                settings.clone(),
                warning,
                renderer.clone(),
            )));
            let (tx, rx) = mpsc::sync_channel(256);
            let origin = Instant::now();
            app.manage(Runtime {
                data: Arc::clone(&data),
                tx: tx.clone(),
                settings_path: settings_path.clone(),
                listener: Mutex::new(None),
                generation: AtomicU64::new(0),
                mutations: Mutex::new(()),
                input_overflow: Arc::new(AtomicBool::new(false)),
            });
            renderer
                .create_window(
                    settings.window_size,
                    settings.always_on_top,
                    move |position| {
                        let _ = tx.try_send(EngineEvent::Moved(position));
                    },
                )
                .map_err(std::io::Error::other)?;
            data.lock()
                .unwrap()
                .initialize_overlay()
                .map_err(std::io::Error::other)?;
            tauri::WebviewWindowBuilder::new(
                app,
                "settings",
                tauri::WebviewUrl::App("index.html?view=settings".into()),
            )
            .title("Tapkin Settings")
            .inner_size(480.0, 650.0)
            .min_inner_size(380.0, 500.0)
            .visible(false)
            .build()?;
            build_tray(app.handle())?;
            let engine_app = app.handle().clone();
            thread::Builder::new()
                .name("tapkin-animation".into())
                .spawn(move || engine(engine_app, data, rx, origin, settings_path))?;
            renderer.set_visible(true).map_err(std::io::Error::other)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = start_listener(&handle);
            });
            let runtime = app.state::<Runtime>();
            if runtime.data.lock().unwrap().warning.is_some() {
                show_settings(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Normal settings-window lifecycle stays in the Tauri shell.
            if window.label() == "settings" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Tapkin could not initialize its desktop windows");
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { ref api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        if let tauri::RunEvent::Exit = event {
            if let Some(runtime) = handle.try_state::<Runtime>() {
                runtime.listener.lock().unwrap().take();
                let _ = flush_settings(&runtime);
                let _ = runtime.tx.try_send(EngineEvent::Quit);
            }
        }
    });
}
