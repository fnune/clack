//! The file behind both the command line and the desktop app, so the two cannot
//! disagree about what Clack is configured to do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

use clack_core::Symbols;

use crate::config::{Anchor, Config, Style};

const MIN_TIMEOUT: f32 = 0.2;
const MAX_TIMEOUT: f32 = 30.0;
const MAX_MARGIN: i32 = 4000;
/// A replacement glyph stands in for one key, so a paragraph pasted into the box
/// is a mistake rather than an intention.
const MAX_GLYPH_CHARS: usize = 8;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub overlay: Overlay,
    pub desktop: Desktop,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Overlay {
    pub font: String,
    /// Distance from the anchored edge, in logical pixels.
    pub margin: i32,
    pub anchor: Anchor,
    /// How long a keystroke stays on screen once typing stops.
    pub timeout_seconds: f32,
    /// An XKB keymap to use instead of the compositor's own.
    pub keymap: Option<PathBuf>,
    /// Whether keys with no letter of their own are drawn as glyphs.
    pub symbols: bool,
    /// Replacements for single keys, by keysym name. An empty string hides one.
    pub symbol_overrides: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Desktop {
    /// Start in the tray, without opening the settings window.
    pub start_hidden: bool,
    /// Start showing keystrokes as soon as the app opens.
    pub start_overlay: bool,
    /// Whether the window wears the desktop's own title bar. Off by default: the
    /// window is one column of settings and a switch, and a title bar above it
    /// says nothing the window does not.
    pub decorations: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        let style = Style::default();

        Self {
            font: style.font,
            margin: style.margin,
            anchor: style.anchor,
            timeout_seconds: Config::default().timeout.as_secs_f32(),
            keymap: None,
            symbols: false,
            symbol_overrides: HashMap::new(),
        }
    }
}

impl Overlay {
    /// Holds every value to something the overlay can be asked to do. Applied on
    /// the way in and on the way out, so a file that has been hand-edited, or
    /// written by an older version, is corrected once rather than carried around
    /// and clamped wherever it happens to be used.
    ///
    /// A number is corrected rather than rejected: someone who typed a margin of
    /// -50 wants the overlay at the edge, and refusing to start over it would be
    /// worse than putting it there.
    pub fn clamped(&self) -> Self {
        let fallback = Self::default();

        Self {
            font: if self.font.trim().is_empty() {
                fallback.font
            } else {
                self.font.clone()
            },
            margin: self.margin.clamp(0, MAX_MARGIN),
            anchor: self.anchor,
            // NaN survives `clamp`, so it cannot be the thing that bounds it.
            timeout_seconds: if self.timeout_seconds.is_finite() {
                self.timeout_seconds.clamp(MIN_TIMEOUT, MAX_TIMEOUT)
            } else {
                fallback.timeout_seconds
            },
            keymap: self.keymap.clone(),
            symbols: self.symbols,
            symbol_overrides: self
                .symbol_overrides
                .iter()
                .map(|(keysym, glyph)| {
                    (
                        keysym.clone(),
                        glyph.chars().take(MAX_GLYPH_CHARS).collect(),
                    )
                })
                .collect(),
        }
    }

    /// Turns what the file said into what a session needs.
    pub fn to_config(&self) -> Config {
        let held = self.clamped();

        Config {
            timeout: Duration::from_secs_f32(held.timeout_seconds),
            style: Style {
                font: held.font.clone(),
                margin: held.margin,
                anchor: held.anchor,
            },
            keymap: held.keymap.clone(),
            symbols: held.to_symbols(),
        }
    }

    pub fn to_symbols(&self) -> Symbols {
        let table = if self.symbols {
            Symbols::glyphs()
        } else {
            Symbols::names()
        };

        table.overridden(&self.symbol_overrides)
    }
}

impl Settings {
    pub fn clamped(&self) -> Self {
        Self {
            overlay: self.overlay.clamped(),
            desktop: self.desktop,
        }
    }

    /// Reads the settings file, or the defaults when there is none. A file that
    /// exists but cannot be understood is an error rather than a silent reset,
    /// because losing someone's configuration without saying so is worse than
    /// refusing to start.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(&path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str::<Self>(&text)
                .map(|settings| settings.clamped())
                .map_err(|error| SettingsError::Invalid(path.display().to_string(), error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SettingsError::Reading(path.display().to_string(), error)),
        }
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)
                .map_err(|error| SettingsError::Writing(path.display().to_string(), error))?;
        }

        let text = toml::to_string_pretty(&self.clamped())?;

        // Written beside the real file and moved onto it, because `fs::write`
        // truncates first: a crash or a second writer part-way through would
        // otherwise leave nothing where someone's settings were. A rename within
        // one directory either happens or does not.
        let partial = beside(path, ".new");
        fs::write(&partial, text)
            .map_err(|error| SettingsError::Writing(partial.display().to_string(), error))?;

        fs::rename(&partial, path)
            .map_err(|error| SettingsError::Writing(path.display().to_string(), error))
    }

    pub fn reset() -> Result<Self, SettingsError> {
        Self::reset_at(&path()?)
    }

    /// Replaces the file with the defaults, keeping what was there alongside it.
    /// The old file is the only record of anything hand-edited that no window
    /// shows, so it is copied before it is lost, and the copy is what the caller
    /// tells the user about.
    pub fn reset_at(path: &Path) -> Result<Self, SettingsError> {
        if path.exists() {
            let backup = backup_for(path);
            fs::copy(path, &backup)
                .map_err(|error| SettingsError::Writing(backup.display().to_string(), error))?;
        }

        let settings = Self::default();
        settings.save_to(path)?;

        Ok(settings)
    }
}

/// `$XDG_CONFIG_HOME/clack/config.toml`, falling back to the default that the
/// XDG base directory specification gives for an unset variable.
pub fn path() -> Result<PathBuf, SettingsError> {
    Ok(config_home()?.join("clack").join("config.toml"))
}

/// Where a reset leaves the settings it replaced.
pub fn backup_path() -> Result<PathBuf, SettingsError> {
    Ok(backup_for(&path()?))
}

fn backup_for(path: &Path) -> PathBuf {
    beside(path, ".bak")
}

/// Appended to the whole path rather than swapping the extension, so
/// `config.toml` becomes `config.toml.bak` and says what it is a copy of. Also
/// keeps the neighbour in the same directory, which a rename onto the original
/// needs in order to be atomic.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut appended = path.as_os_str().to_owned();
    appended.push(suffix);

    PathBuf::from(appended)
}

fn config_home() -> Result<PathBuf, SettingsError> {
    match env::var_os("XDG_CONFIG_HOME") {
        Some(configured) if !configured.is_empty() => Ok(PathBuf::from(configured)),
        _ => Ok(PathBuf::from(env::var_os("HOME").ok_or(SettingsError::NoHome)?).join(".config")),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("neither XDG_CONFIG_HOME nor HOME is set, so there is nowhere to keep settings")]
    NoHome,
    #[error("reading {0} failed: {1}")]
    Reading(String, #[source] io::Error),
    #[error("writing {0} failed: {1}")]
    Writing(String, #[source] io::Error),
    #[error("{0} is not valid: {1}")]
    Invalid(String, #[source] toml::de::Error),
    #[error("the settings could not be encoded: {0}")]
    Encoding(#[from] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let missing = Path::new("/nonexistent/clack/config.toml");

        assert_eq!(Settings::load_from(missing).unwrap(), Settings::default());
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_everything_else() {
        let settings: Settings = toml::from_str("[overlay]\nmargin = 12\n").unwrap();

        assert_eq!(settings.overlay.margin, 12);
        assert_eq!(settings.overlay.font, Overlay::default().font);
        assert_eq!(settings.desktop, Desktop::default());
    }

    #[test]
    fn an_unknown_key_is_reported_rather_than_ignored() {
        let outcome = toml::from_str::<Settings>("[overlay]\nfont-size = 12\n");

        assert!(outcome.is_err(), "a typo should not be silently dropped");
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        let settings = Settings {
            overlay: Overlay {
                font: "Iosevka 30".to_owned(),
                margin: 40,
                anchor: Anchor::Top,
                timeout_seconds: 2.5,
                keymap: Some(PathBuf::from("/tmp/layout.xkb")),
                symbols: true,
                symbol_overrides: HashMap::from([("Tab".to_owned(), "→".to_owned())]),
            },
            desktop: Desktop {
                start_hidden: true,
                start_overlay: true,
                decorations: true,
            },
        };

        let text = toml::to_string_pretty(&settings).unwrap();

        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn a_backup_says_what_it_is_a_backup_of() {
        assert_eq!(
            backup_for(Path::new("/home/someone/.config/clack/config.toml")),
            Path::new("/home/someone/.config/clack/config.toml.bak")
        );
    }

    #[test]
    fn a_reset_keeps_what_it_replaced() {
        let directory = std::env::temp_dir().join("clack-reset-test");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::write(&path, "[overlay]\nmargin = 12\n").unwrap();

        let reset = Settings::reset_at(&path).unwrap();

        assert_eq!(reset, Settings::default());
        assert_eq!(
            Settings::load_from(&backup_for(&path))
                .unwrap()
                .overlay
                .margin,
            12
        );

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_reset_with_nothing_to_replace_writes_the_defaults_anyway() {
        let directory = std::env::temp_dir().join("clack-reset-fresh-test");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");

        assert_eq!(Settings::reset_at(&path).unwrap(), Settings::default());
        assert!(!backup_for(&path).exists());

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn saving_leaves_nothing_beside_the_file_it_wrote() {
        let directory = std::env::temp_dir().join("clack-atomic-test");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");

        Settings::default().save_to(&path).unwrap();

        assert!(path.exists());
        assert!(
            !beside(&path, ".new").exists(),
            "the half-written neighbour should have been renamed, not left"
        );
        assert_eq!(Settings::load_from(&path).unwrap(), Settings::default());

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_negative_distance_becomes_no_distance() {
        let overlay = Overlay {
            margin: -50,
            ..Overlay::default()
        };

        assert_eq!(overlay.clamped().margin, 0);
    }

    #[test]
    fn a_timeout_that_is_not_a_number_falls_back_rather_than_surviving_the_clamp() {
        let overlay = Overlay {
            timeout_seconds: f32::NAN,
            ..Overlay::default()
        };

        assert_eq!(
            overlay.clamped().timeout_seconds,
            Overlay::default().timeout_seconds
        );
    }

    #[test]
    fn a_blank_font_falls_back_to_one_that_exists() {
        let overlay = Overlay {
            font: "   ".to_owned(),
            ..Overlay::default()
        };

        assert_eq!(overlay.clamped().font, Overlay::default().font);
    }

    #[test]
    fn a_replacement_glyph_cannot_be_a_paragraph() {
        let overlay = Overlay {
            symbol_overrides: HashMap::from([("Tab".to_owned(), "x".repeat(400))]),
            ..Overlay::default()
        };

        assert_eq!(
            overlay
                .clamped()
                .symbol_overrides
                .get("Tab")
                .map(String::len),
            Some(MAX_GLYPH_CHARS)
        );
    }

    #[test]
    fn a_file_out_of_range_is_corrected_on_the_way_in_and_out() {
        let directory = std::env::temp_dir().join("clack-clamp-test");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::write(&path, "[overlay]\nmargin = -50\ntimeout_seconds = 900\n").unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.overlay.margin, 0);
        assert_eq!(loaded.overlay.timeout_seconds, MAX_TIMEOUT);

        Settings {
            overlay: Overlay {
                margin: -50,
                ..Overlay::default()
            },
            ..Settings::default()
        }
        .save_to(&path)
        .unwrap();

        assert!(
            !fs::read_to_string(&path).unwrap().contains("-50"),
            "an out-of-range value should not reach the file"
        );

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_timeout_the_overlay_could_not_honour_is_held_in_range() {
        let overlay = Overlay {
            timeout_seconds: 1e9,
            ..Overlay::default()
        };

        assert_eq!(
            overlay.to_config().timeout,
            Duration::from_secs_f32(MAX_TIMEOUT)
        );
    }
}
