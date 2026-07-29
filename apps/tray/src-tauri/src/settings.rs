use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

const MIN_TIMEOUT: f32 = 0.2;
const MAX_TIMEOUT: f32 = 30.0;
const MAX_MARGIN: i32 = 4000;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    pub font: String,
    pub margin: i32,
    pub timeout_seconds: f32,
    pub anchor: Anchor,
    /// Start in the tray, without opening this window.
    pub start_hidden: bool,
    /// Start showing keystrokes as soon as the app opens.
    pub start_overlay: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let defaults = clack::Style::default();

        Self {
            font: defaults.font,
            margin: defaults.margin,
            timeout_seconds: clack::Config::default().timeout.as_secs_f32(),
            anchor: Anchor::Bottom,
            start_hidden: false,
            start_overlay: false,
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        match fs::read_to_string(path()?) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SettingsError::Reading(error)),
        }
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        let path = path()?;
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(SettingsError::Writing)?;
        }

        fs::write(path, toml::to_string_pretty(self)?).map_err(SettingsError::Writing)
    }

    pub fn to_config(&self) -> clack::Config {
        clack::Config {
            timeout: Duration::from_secs_f32(self.timeout_seconds.clamp(MIN_TIMEOUT, MAX_TIMEOUT)),
            style: clack::Style {
                font: self.font.clone(),
                margin: self.margin.clamp(0, MAX_MARGIN),
                anchor: match self.anchor {
                    Anchor::Top => clack::Anchor::Top,
                    Anchor::Bottom => clack::Anchor::Bottom,
                },
            },
        }
    }
}

fn path() -> Result<PathBuf, SettingsError> {
    let base = match env::var_os("XDG_CONFIG_HOME") {
        Some(configured) if !configured.is_empty() => PathBuf::from(configured),
        _ => PathBuf::from(env::var_os("HOME").ok_or(SettingsError::NoHome)?).join(".config"),
    };

    Ok(base.join("clack").join("tray.toml"))
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("neither XDG_CONFIG_HOME nor HOME is set, so there is nowhere to keep settings")]
    NoHome,
    #[error("reading the settings file failed: {0}")]
    Reading(#[source] io::Error),
    #[error("writing the settings file failed: {0}")]
    Writing(#[source] io::Error),
    #[error("the settings file is not valid: {0}")]
    Invalid(#[from] toml::de::Error),
    #[error("the settings could not be encoded: {0}")]
    Encoding(#[from] toml::ser::Error),
}
