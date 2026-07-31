#![cfg_attr(test, allow(clippy::expect_used, clippy::panic, clippy::unwrap_used))]

mod appearance;
mod logo;
mod supervisor;
mod tray;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use clack::Settings;

use crate::appearance::Appearance;
use crate::logo::Mood;
use crate::supervisor::Supervisor;

const STATUS_EVENT: &str = "clack://status";
const APPEARANCE_EVENT: &str = "clack://appearance";

#[derive(Default)]
pub struct AppState {
    supervisor: Mutex<Supervisor>,
    /// Whether the window is still owed its first appearance.
    reveal: Mutex<bool>,
    /// Why the last attempt to show keys did not work. The session itself only
    /// remembers a failure for as long as it is running, and the tray has to be
    /// able to say something went wrong with no window open to say it in.
    trouble: Mutex<Option<String>>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub failure: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsFile {
    pub path: String,
    /// Whether Clack may write to it. A file Nix generates is a link into a
    /// read-only store, so edits hold for the session and no longer.
    pub writable: bool,
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
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            reset_settings,
            settings_backup_path,
            settings_file,
            ready,
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

            tray::install(app.handle())?;

            appearance::watch({
                let app = app.handle().clone();
                move |appearance| {
                    let dark = appearance.dark.unwrap_or(true);
                    let _ = app.run_on_main_thread(move || tray::shade(dark));
                    let _ = app.emit(APPEARANCE_EVENT, appearance);
                }
            });

            if settings.desktop.start_overlay {
                let handle = app.handle().clone();
                if let Err(error) = start_session(&handle, &settings) {
                    eprintln!("clack-desktop: {error}");
                }
            }

            // The window is built undecorated, so only the other direction is
            // worth acting on.
            if settings.desktop.decorations {
                dress_window(app.handle(), true);
            }

            if let Ok(mut reveal) = app.state::<AppState>().reveal.lock() {
                *reveal = !settings.desktop.start_hidden;
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
                app.run(|handle, event| match event {
                    // Whatever the launch left behind, said in the tray. Starting
                    // hidden means this is the only place it gets said at all.
                    RunEvent::Ready => publish(handle),
                    RunEvent::ExitRequested { .. } => with_supervisor(handle, Supervisor::stop),
                    _ => {}
                });
            },
        );
}

pub(crate) fn label(running: bool) -> &'static str {
    if running {
        "Stop showing keys"
    } else {
        "Start showing keys"
    }
}

pub(crate) fn quit(app: &AppHandle) {
    with_supervisor(app, Supervisor::stop);
    app.exit(0);
}

/// Whether the window wears the desktop's own title bar. The web view draws its
/// own strip when it does not, so the two are never both there.
fn dress_window(app: &AppHandle, decorated: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_decorations(decorated);
    }
}

pub(crate) fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn toggle_session(app: &AppHandle) {
    let running = read_status(app).running;

    let outcome = if running {
        stop_session(app);
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
    let outcome = state
        .supervisor
        .lock()
        .map_err(|_| "the session lock is poisoned".to_owned())
        .and_then(|mut supervisor| {
            supervisor
                .start(settings.overlay.to_config())
                .map_err(|error| error.to_string())
        });

    remember(app, outcome.as_ref().err().cloned());

    outcome
}

/// Stopping is an answer, not a fault: whatever went wrong last time is no longer
/// what Clack is doing, so the tray goes back to idle rather than staying broken.
fn stop_session(app: &AppHandle) {
    with_supervisor(app, Supervisor::stop);
    remember(app, None);
}

fn with_supervisor(app: &AppHandle, action: impl FnOnce(&mut Supervisor)) {
    if let Ok(mut supervisor) = app.state::<AppState>().supervisor.lock() {
        action(&mut supervisor);
    }
}

fn remember(app: &AppHandle, trouble: Option<String>) {
    if let Ok(mut slot) = app.state::<AppState>().trouble.lock() {
        *slot = trouble;
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

/// Keeps the window, the tray icon and the tray menu agreeing about what is
/// happening.
fn publish(app: &AppHandle) {
    let status = read_status(app);
    let mood = describe(app, &status);
    let running = status.running;

    // The tray is GTK, and GTK belongs to the thread running the loop. A command
    // from the window does not arrive on it.
    let _ = app.run_on_main_thread(move || tray::wear(mood, running));

    let _ = app.emit(STATUS_EVENT, status);
}

/// A failure the session reported outlives the session that reported it, so both
/// it and what was remembered are asked.
fn describe(app: &AppHandle, status: &Status) -> Mood {
    let troubled = app
        .state::<AppState>()
        .trouble
        .lock()
        .is_ok_and(|slot| slot.is_some());

    if status.failure.is_some() || troubled {
        Mood::Stuck
    } else if status.running {
        Mood::Showing
    } else {
        Mood::Idle
    }
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

/// Where the settings live and whether they are Clack's to change, so the window
/// can say which of the two it is doing rather than reporting a failure for a
/// file that was never meant to be written.
#[tauri::command]
fn settings_file() -> Result<SettingsFile, String> {
    let path = clack::settings::path().map_err(|error| error.to_string())?;

    Ok(SettingsFile {
        writable: clack::settings::is_writable_at(&path),
        path: path.display().to_string(),
    })
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    dress_window(&app, settings.desktop.decorations);
    keep(&settings)
}

/// A file someone else generates is left as it is. The change still holds for as
/// long as Clack is running, which is what the window says it does.
fn keep(settings: &Settings) -> Result<(), String> {
    let path = clack::settings::path().map_err(|error| error.to_string())?;

    if !clack::settings::is_writable_at(&path) {
        return Ok(());
    }

    settings.save_to(&path).map_err(|error| error.to_string())
}

/// Where a reset would leave the settings it replaces, so the window can name it
/// before asking rather than after.
#[tauri::command]
fn settings_backup_path() -> Result<String, String> {
    clack::settings::backup_path()
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

/// With a file Clack may not write there is nothing to replace and nothing to
/// keep a copy of, so the defaults are handed back for the session instead.
#[tauri::command]
fn reset_settings(app: AppHandle) -> Result<Settings, String> {
    let path = clack::settings::path().map_err(|error| error.to_string())?;

    let settings = if clack::settings::is_writable_at(&path) {
        Settings::reset_at(&path).map_err(|error| error.to_string())?
    } else {
        Settings::default()
    };

    dress_window(&app, settings.desktop.decorations);

    Ok(settings)
}

/// The window is created hidden and shown from here, once the page has drawn
/// itself. Showing it any earlier shows an empty transparent one: a black box, and
/// then a flash as the page arrives over it.
#[tauri::command]
fn ready(app: AppHandle) {
    let owed = app
        .state::<AppState>()
        .reveal
        .lock()
        .map(|mut reveal| std::mem::replace(&mut *reveal, false))
        .unwrap_or(false);

    if owed {
        show_window(&app);
    }
}

#[tauri::command]
fn start(app: AppHandle, settings: Settings) -> Result<Status, String> {
    keep(&settings)?;
    start_session(&app, &settings)?;
    publish(&app);

    Ok(read_status(&app))
}

#[tauri::command]
fn stop(app: AppHandle) -> Status {
    stop_session(&app);
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
    stop_session(&app);
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
