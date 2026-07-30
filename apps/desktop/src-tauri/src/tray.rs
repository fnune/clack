//! The tray icon, spoken to directly rather than through Tauri's tray.
//!
//! An indicator publishes its icon as a *name* and the panel resolves that name
//! against a directory. Tauri's tray instead writes each drawing to a new file and
//! publishes its absolute path, and the indicator only ever announces the first of
//! those paths: every later drawing lands in a file nobody reads, and deleting the
//! one before it leaves the panel holding a path to nothing. Three files in a
//! directory of Clack's own, and a name to choose between them, is the way this was
//! meant to be done.

use std::cell::RefCell;
use std::path::PathBuf;

use gtk::prelude::{GtkMenuItemExt, MenuShellExt, WidgetExt};
use libappindicator::{AppIndicator, AppIndicatorStatus};
use tauri::AppHandle;

use crate::logo::{self, Ink, Mood};

/// The size the icons are drawn at: twice the 22 pixels a panel usually gives a
/// tray icon, so a panel drawn at twice the scale has something to work from.
const SIZE: u32 = 44;

thread_local! {
    /// GTK objects belong to the thread running the loop, which rules out Tauri's
    /// state: everything there is shared between threads.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

struct Tray {
    indicator: AppIndicator,
    toggle: gtk::MenuItem,
    mood: Mood,
    ink: Ink,
    /// The indicator draws the menu but does not own it.
    _menu: gtk::Menu,
}

#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("{0}")]
    Logo(#[from] logo::LogoError),
    #[error("the icons have nowhere to live: {0}")]
    Home(#[from] std::io::Error),
}

/// One icon per state per colour. The colour is in the name because changing the
/// name is the only thing a panel treats as a new icon: rewriting the file it
/// already read leaves it drawing what it read the first time.
fn icon(mood: Mood, ink: Ink) -> String {
    let state = match mood {
        Mood::Idle => "idle",
        Mood::Showing => "showing",
        Mood::Stuck => "stuck",
    };
    let shade = match ink {
        Ink::Light => "light",
        Ink::Dark => "dark",
    };

    format!("clack-{state}-{shade}")
}

fn ink(dark: bool) -> Ink {
    if dark { Ink::Light } else { Ink::Dark }
}

/// Draws every state up front, so that changing state later is a name and nothing
/// else.
pub fn install(app: &AppHandle) -> Result<(), TrayError> {
    let home = home()?;
    std::fs::create_dir_all(&home)?;

    for mood in [Mood::Idle, Mood::Showing, Mood::Stuck] {
        for shade in [Ink::Light, Ink::Dark] {
            let pixels = logo::draw(mood, SIZE, shade)?;
            logo::write(
                &home.join(format!("{}.png", icon(mood, shade))),
                &pixels,
                SIZE,
            )?;
        }
    }

    let ink = ink(crate::appearance::read().dark.unwrap_or(true));
    let mut indicator =
        AppIndicator::with_path("clack", &icon(Mood::Idle, ink), &home.to_string_lossy());
    indicator.set_status(AppIndicatorStatus::Active);

    let mut menu = gtk::Menu::new();
    let show = gtk::MenuItem::with_label("Show settings");
    let toggle = gtk::MenuItem::with_label(crate::label(false));
    let separator = gtk::SeparatorMenuItem::new();
    let quit = gtk::MenuItem::with_label("Quit");

    show.connect_activate({
        let app = app.clone();
        move |_| crate::show_window(&app)
    });
    toggle.connect_activate({
        let app = app.clone();
        move |_| crate::toggle_session(&app)
    });
    quit.connect_activate({
        let app = app.clone();
        move |_| crate::quit(&app)
    });

    menu.append(&show);
    menu.append(&toggle);
    menu.append(&separator);
    menu.append(&quit);
    menu.show_all();
    indicator.set_menu(&mut menu);

    TRAY.with_borrow_mut(|slot| {
        *slot = Some(Tray {
            indicator,
            toggle,
            mood: Mood::Idle,
            ink,
            _menu: menu,
        });
    });

    Ok(())
}

/// Only the name of the icon changes, which is what an indicator announces and a
/// panel listens for.
pub fn wear(mood: Mood, running: bool) {
    TRAY.with_borrow_mut(|slot| {
        let Some(tray) = slot.as_mut() else {
            return;
        };

        tray.mood = mood;
        tray.indicator.set_icon_full(&icon(mood, tray.ink), "Clack");
        tray.toggle.set_label(crate::label(running));
    });
}

/// Follows the desktop from light to dark and back. The mark carries no colour of
/// its own, and the one the panel paints with changes under it.
pub fn shade(dark: bool) {
    TRAY.with_borrow_mut(|slot| {
        let Some(tray) = slot.as_mut() else {
            return;
        };

        tray.ink = ink(dark);
        tray.indicator
            .set_icon_full(&icon(tray.mood, tray.ink), "Clack");
    });
}

/// Somewhere of Clack's own, so nothing else owns the files the panel is reading.
fn home() -> Result<PathBuf, std::io::Error> {
    Ok(std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("clack-tray"))
}
