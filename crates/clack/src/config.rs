use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    pub font: String,
    /// Distance from the anchored edge, in logical pixels.
    pub margin: i32,
    pub anchor: Anchor,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font: "Sans Bold 24".to_owned(),
            margin: 96,
            anchor: Anchor::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// How long a keystroke stays on screen once typing stops.
    pub timeout: Duration,
    pub style: Style,
    /// An XKB keymap to use instead of the compositor's own. The compositor's
    /// is the right answer almost always, since it is what the user configured.
    pub keymap: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1500),
            style: Style::default(),
            keymap: None,
        }
    }
}
