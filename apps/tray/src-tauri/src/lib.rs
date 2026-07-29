mod settings;
mod supervisor;

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent, Wry};

use crate::settings::Settings;
use crate::supervisor::Supervisor;

const SHOW: &str = "show";
const TOGGLE: &str = "toggle";
const QUIT: &str = "quit";
const STATUS_EVENT: &str = "clack://status";

#[derive(Default)]
pub struct AppState {
    supervisor: Mutex<Supervisor>,
    toggle: Mutex<Option<MenuItem<Wry>>>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub paused: bool,
    pub failure: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub keyboards: usize,
    pub denied: usize,
    pub layer_shell: bool,
    pub keymap: bool,
    pub usable: bool,
    pub summary: String,
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            start,
            stop,
            set_paused,
            status,
            diagnose,
        ])
        .setup(|app| {
            let settings = Settings::load().unwrap_or_else(|error| {
                eprintln!("clack-tray: {error}, falling back to defaults");
                Settings::default()
            });

            install_tray(app.handle())?;

            if settings.start_overlay {
                let handle = app.handle().clone();
                if let Err(error) = start_session(&handle, &settings) {
                    eprintln!("clack-tray: {error}");
                }
            }

            if !settings.start_hidden {
                show_window(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .map_or_else(
            |error| eprintln!("clack-tray: {error}"),
            |app| {
                app.run(|handle, event| {
                    if matches!(event, RunEvent::ExitRequested { .. }) {
                        with_supervisor(handle, Supervisor::stop);
                    }
                });
            },
        );
}

fn install_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW, "Show settings", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, TOGGLE, "Start showing keys", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &toggle, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    if let Ok(mut slot) = app.state::<AppState>().toggle.lock() {
        *slot = Some(toggle);
    }

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::AssetNotFound("the tray icon is missing from the bundle".to_owned())
        })?)
        .tooltip("Clack")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            SHOW => show_window(app),
            TOGGLE => toggle_session(app),
            QUIT => {
                with_supervisor(app, Supervisor::stop);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_session(app: &AppHandle) {
    let running = read_status(app).running;

    let outcome = if running {
        with_supervisor(app, Supervisor::stop);
        Ok(())
    } else {
        match Settings::load() {
            Ok(settings) => start_session(app, &settings),
            Err(error) => Err(error.to_string()),
        }
    };

    if let Err(error) = outcome {
        eprintln!("clack-tray: {error}");
    }

    publish(app);
}

fn start_session(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut supervisor = state
        .supervisor
        .lock()
        .map_err(|_| "the session lock is poisoned".to_owned())?;

    supervisor
        .start(settings.to_config())
        .map_err(|error| error.to_string())
}

fn with_supervisor(app: &AppHandle, action: impl FnOnce(&mut Supervisor)) {
    if let Ok(mut supervisor) = app.state::<AppState>().supervisor.lock() {
        action(&mut supervisor);
    }
}

fn read_status(app: &AppHandle) -> Status {
    let state = app.state::<AppState>();
    let Ok(supervisor) = state.supervisor.lock() else {
        return Status {
            failure: Some("the session lock is poisoned".to_owned()),
            ..Status::default()
        };
    };

    Status {
        running: supervisor
            .state()
            .is_some_and(|state| state != clack::State::Stopped),
        paused: supervisor.state() == Some(clack::State::Paused),
        failure: supervisor.failure(),
    }
}

/// Keeps the window and the tray menu agreeing about what is happening.
fn publish(app: &AppHandle) {
    let status = read_status(app);

    if let Ok(toggle) = app.state::<AppState>().toggle.lock()
        && let Some(item) = toggle.as_ref()
    {
        let label = if status.running {
            "Stop showing keys"
        } else {
            "Start showing keys"
        };
        let _ = item.set_text(label);
    }

    let _ = app.emit(STATUS_EVENT, status);
}

#[tauri::command]
fn load_settings() -> Result<Settings, String> {
    Settings::load().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    settings.save().map_err(|error| error.to_string())
}

#[tauri::command]
fn start(app: AppHandle, settings: Settings) -> Result<Status, String> {
    settings.save().map_err(|error| error.to_string())?;
    start_session(&app, &settings)?;
    publish(&app);

    Ok(read_status(&app))
}

#[tauri::command]
fn stop(app: AppHandle) -> Status {
    with_supervisor(&app, Supervisor::stop);
    publish(&app);

    read_status(&app)
}

#[tauri::command]
fn set_paused(app: AppHandle, paused: bool, state: State<AppState>) -> Result<Status, String> {
    state
        .supervisor
        .lock()
        .map_err(|_| "the session lock is poisoned".to_owned())?
        .set_paused(paused);
    publish(&app);

    Ok(read_status(&app))
}

#[tauri::command]
fn status(app: AppHandle) -> Status {
    read_status(&app)
}

#[tauri::command]
fn diagnose() -> Result<Diagnosis, String> {
    let report = clack::Report::gather().map_err(|error| error.to_string())?;

    Ok(Diagnosis {
        keyboards: report.keyboards(),
        denied: report.denied(),
        layer_shell: report.can_draw(),
        keymap: report.can_translate(),
        usable: report.is_usable(),
        summary: summarise(&report),
    })
}

fn summarise(report: &clack::Report) -> String {
    if report.is_usable() {
        return format!("Ready. {} keyboards readable.", report.keyboards());
    }

    if report.keyboards() == 0 {
        return format!(
            "No readable keyboard, {} devices denied. Install the udev rule from packaging/65-clack.rules.",
            report.denied()
        );
    }

    if !report.can_draw() {
        return "This compositor does not support layer shell, so the overlay cannot be drawn."
            .to_owned();
    }

    "The compositor did not send a keymap, so keys cannot be named.".to_owned()
}
