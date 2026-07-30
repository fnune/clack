mod appearance;
mod supervisor;

use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent, Wry};

use clack::Settings;

use crate::appearance::Appearance;
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
    /// Whether the udev rule is in place. The window offers whichever direction
    /// is left, rather than hiding the one that is done.
    pub rule_installed: bool,
    pub rule_path: String,
    pub rule_text: String,
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            reset_settings,
            settings_backup_path,
            start,
            stop,
            status,
            diagnose,
            grant_access,
            revoke_access,
            desktop_appearance,
            preview,
        ])
        .setup(|app| {
            let settings = Settings::load().unwrap_or_else(|error| {
                eprintln!("clack-desktop: {error}, falling back to defaults");
                Settings::default()
            });

            install_tray(app.handle())?;

            if settings.desktop.start_overlay {
                let handle = app.handle().clone();
                if let Err(error) = start_session(&handle, &settings) {
                    eprintln!("clack-desktop: {error}");
                }
            }

            if !settings.desktop.start_hidden {
                show_window(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window leaves Clack in the tray. Without preventing
            // the close the window is destroyed, and with it the last window,
            // which ends the process.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .map_or_else(
            |error| eprintln!("clack-desktop: {error}"),
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
        eprintln!("clack-desktop: {error}");
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
        .start(settings.overlay.to_config())
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

/// Keys worth offering a replacement for. Everything else is reachable from the
/// config file.
const EDITABLE: [(&str, &str); 4] = [
    (clack::SPACE, "Space"),
    ("Return", "Return"),
    ("Tab", "Tab"),
    ("BackSpace", "Backspace"),
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub runs: Vec<PreviewRun>,
    pub symbols: Vec<SymbolEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRun {
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolEntry {
    pub keysym: String,
    pub label: String,
    pub glyph: String,
}

/// Runs a sample through the same model the overlay uses, so the window cannot
/// disagree with the thing it is previewing.
#[tauri::command]
fn preview(settings: Settings) -> Preview {
    let symbols = settings.overlay.to_symbols();
    let mut model = clack::Model::new(std::time::Duration::from_secs(60), symbols.clone());

    for character in "hello world".chars() {
        let keysym = if character == ' ' {
            clack::SPACE.to_owned()
        } else {
            character.to_string()
        };
        model.record(
            &clack::Translated {
                text: Some(character.to_string()),
                keysym,
                modifiers: clack::Modifiers::default(),
            },
            clack::Timestamp::ZERO,
        );
    }

    for (keysym, text) in [("Return", Some("\r")), ("Tab", Some("\t"))] {
        model.record(
            &clack::Translated {
                text: text.map(str::to_owned),
                keysym: keysym.to_owned(),
                modifiers: clack::Modifiers::default(),
            },
            clack::Timestamp::ZERO,
        );
    }

    model.record(
        &clack::Translated {
            text: None,
            keysym: "s".to_owned(),
            modifiers: clack::Modifiers {
                ctrl: true,
                ..clack::Modifiers::default()
            },
        },
        clack::Timestamp::ZERO,
    );

    Preview {
        runs: model
            .frame()
            .runs
            .into_iter()
            .map(|run| PreviewRun {
                text: run.text,
                kind: match run.kind {
                    clack::RunKind::Text => "text",
                    clack::RunKind::Chord => "chord",
                    clack::RunKind::Special => "special",
                }
                .to_owned(),
            })
            .collect(),
        // The table says what the symbols are; the switch says whether they are
        // used. Showing names here conflated the two.
        symbols: {
            let table = clack::Symbols::glyphs().overridden(&settings.overlay.symbol_overrides);

            EDITABLE
                .into_iter()
                .map(|(keysym, label)| SymbolEntry {
                    keysym: keysym.to_owned(),
                    label: label.to_owned(),
                    glyph: table.glyph(keysym).unwrap_or_default().to_owned(),
                })
                .collect()
        },
    }
}

#[tauri::command]
fn desktop_appearance() -> Appearance {
    appearance::read()
}

#[tauri::command]
fn load_settings() -> Result<Settings, String> {
    Settings::load().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    settings.save().map_err(|error| error.to_string())
}

/// Where a reset would leave the settings it replaces, so the window can name it
/// before asking rather than after.
#[tauri::command]
fn settings_backup_path() -> Result<String, String> {
    clack::settings::backup_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_settings() -> Result<Settings, String> {
    Settings::reset().map_err(|error| error.to_string())
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
        rule_installed: clack::access::status() == clack::RuleStatus::Installed,
        rule_path: clack::access::rule_path().display().to_string(),
        rule_text: clack::access::rule_text().to_owned(),
    })
}

/// Installs the udev rule through polkit, which prompts once and leaves nothing
/// elevated behind.
#[tauri::command]
fn grant_access() -> Result<Diagnosis, String> {
    change_access(clack::access::Change::Allow)
}

/// Deletes it again, the same way. A session already running holds its devices
/// open, so it is stopped first: carrying on reading keys after the permission to
/// do so was withdrawn is not something to leave running.
#[tauri::command]
fn revoke_access(app: AppHandle) -> Result<Diagnosis, String> {
    with_supervisor(&app, Supervisor::stop);
    publish(&app);

    change_access(clack::access::Change::Revoke)
}

/// Closing the prompt is an answer, not a fault, so it reports the state it found
/// rather than an error the window would show in red.
fn change_access(change: clack::access::Change) -> Result<Diagnosis, String> {
    match clack::access::elevate(change) {
        Ok(()) | Err(clack::AccessError::Declined(_)) => diagnose(),
        Err(error) => Err(error.to_string()),
    }
}

fn summarise(report: &clack::Report) -> String {
    if report.is_usable() {
        return format!("Ready. {} keyboards readable.", report.keyboards());
    }

    if report.keyboards() == 0 {
        return format!(
            "Clack cannot read any keyboard, and {} devices refused permission.",
            report.denied()
        );
    }

    if !report.can_draw() {
        return "This desktop cannot show the overlay above other windows.".to_owned();
    }

    "This desktop did not share its keyboard layout, so keys cannot be named.".to_owned()
}
