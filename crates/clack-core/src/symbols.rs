use std::collections::HashMap;

/// The name Clack uses for the space bar, which xkb calls `space`.
pub const SPACE: &str = "space";

/// How a key with no letter of its own is written. Either by name, which is
/// what a keymap already calls it, or by a glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbols {
    glyphs: HashMap<String, String>,
}

impl Symbols {
    /// Keys keep the names the keymap gives them, and a space stays a space.
    pub fn names() -> Self {
        Self {
            glyphs: HashMap::new(),
        }
    }

    /// Every key that produces nothing visible gets one, including the space,
    /// so a recording shows exactly what was typed.
    pub fn glyphs() -> Self {
        let pairs = [
            (SPACE, "␣"),
            ("Return", "⏎"),
            ("KP_Enter", "⏎"),
            ("Tab", "⇥"),
            ("ISO_Left_Tab", "⇤"),
            ("BackSpace", "⌫"),
            ("Delete", "⌦"),
            ("Escape", "⎋"),
            ("Up", "↑"),
            ("Down", "↓"),
            ("Left", "←"),
            ("Right", "→"),
            ("Home", "↖"),
            ("End", "↘"),
            ("Prior", "⇞"),
            ("Next", "⇟"),
        ];

        Self {
            glyphs: pairs
                .into_iter()
                .map(|(key, glyph)| (key.to_owned(), glyph.to_owned()))
                .collect(),
        }
    }

    /// Replaces individual entries. An empty replacement hides the key.
    pub fn overridden(mut self, overrides: &HashMap<String, String>) -> Self {
        for (key, glyph) in overrides {
            self.glyphs.insert(key.clone(), glyph.clone());
        }

        self
    }

    /// What to show for a key, or `None` when it has nothing of its own and the
    /// caller should fall back to the keysym name.
    pub fn glyph(&self, keysym: &str) -> Option<&str> {
        self.glyphs.get(keysym).map(String::as_str)
    }

    pub fn hides(&self, keysym: &str) -> bool {
        self.glyphs.get(keysym).is_some_and(String::is_empty)
    }
}

impl Default for Symbols {
    fn default() -> Self {
        Self::names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_leave_every_key_to_its_keysym() {
        assert_eq!(Symbols::names().glyph("Return"), None);
        assert_eq!(Symbols::names().glyph(SPACE), None);
    }

    #[test]
    fn glyphs_cover_the_keys_that_show_nothing_of_their_own() {
        let symbols = Symbols::glyphs();

        assert_eq!(symbols.glyph("Return"), Some("⏎"));
        assert_eq!(symbols.glyph(SPACE), Some("␣"));
        assert_eq!(symbols.glyph("a"), None, "a letter writes itself");
    }

    #[test]
    fn an_override_replaces_one_entry_and_leaves_the_rest() {
        let overrides = HashMap::from([("Tab".to_owned(), "→".to_owned())]);
        let symbols = Symbols::glyphs().overridden(&overrides);

        assert_eq!(symbols.glyph("Tab"), Some("→"));
        assert_eq!(symbols.glyph("Return"), Some("⏎"));
    }

    #[test]
    fn an_empty_override_hides_the_key() {
        let overrides = HashMap::from([(SPACE.to_owned(), String::new())]);
        let symbols = Symbols::glyphs().overridden(&overrides);

        assert!(symbols.hides(SPACE));
    }
}
