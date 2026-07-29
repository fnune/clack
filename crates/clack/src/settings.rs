//! The file behind both the command line and the tray, so the two cannot
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub overlay: Overlay,
    pub tray: Tray,
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
pub struct Tray {
    /// Start in the tray, without opening the settings window.
    pub start_hidden: bool,
    /// Start showing keystrokes as soon as the tray app opens.
    pub start_overlay: bool,
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
    /// Turns what the file said into what a session needs, holding values to
    /// what can actually be drawn.
    pub fn to_config(&self) -> Config {
        Config {
            timeout: Duration::from_secs_f32(self.timeout_seconds.clamp(MIN_TIMEOUT, MAX_TIMEOUT)),
            style: Style {
                font: self.font.clone(),
                margin: self.margin.clamp(0, MAX_MARGIN),
                anchor: self.anchor,
            },
            keymap: self.keymap.clone(),
            symbols: self.to_symbols(),
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
    /// Reads the settings file, or the defaults when there is none. A file that
    /// exists but cannot be understood is an error rather than a silent reset,
    /// because losing someone's configuration without saying so is worse than
    /// refusing to start.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(&path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
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

        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)
            .map_err(|error| SettingsError::Writing(path.display().to_string(), error))
    }
}

/// `$XDG_CONFIG_HOME/clack/config.toml`, falling back to the default that the
/// XDG base directory specification gives for an unset variable.
pub fn path() -> Result<PathBuf, SettingsError> {
    Ok(config_home()?.join("clack").join("config.toml"))
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
        assert_eq!(settings.tray, Tray::default());
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
            tray: Tray {
                start_hidden: true,
                start_overlay: true,
            },
        };

        let text = toml::to_string_pretty(&settings).unwrap();

        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
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
