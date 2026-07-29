use core::time::Duration;
use std::collections::VecDeque;

use crate::clock::Timestamp;
use crate::frame::{Frame, Run, RunKind};
use crate::key::Translated;
use crate::symbols::Symbols;

#[derive(Clone, Debug)]
struct Entry {
    run: Run,
    at: Timestamp,
}

const MAX_RUNS: usize = 48;

#[derive(Clone, Debug)]
pub struct Model {
    timeout: Duration,
    symbols: Symbols,
    entries: VecDeque<Entry>,
}

impl Model {
    pub fn new(timeout: Duration, symbols: Symbols) -> Self {
        Self {
            timeout,
            symbols,
            entries: VecDeque::new(),
        }
    }

    pub fn record(&mut self, translated: &Translated, now: Timestamp) {
        let Some(run) = to_run(translated, &self.symbols) else {
            return;
        };

        self.entries.push_back(Entry { run, at: now });

        while self.entries.len() > MAX_RUNS {
            self.entries.pop_front();
        }
    }

    /// The whole line clears at once. Expiring entry by entry would erode the
    /// line from the left while the reader is still reading it.
    pub fn expire(&mut self, now: Timestamp) {
        let idle = self
            .entries
            .back()
            .is_some_and(|newest| now.elapsed_since(newest.at) >= self.timeout);

        if idle {
            self.entries.clear();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn frame(&self) -> Frame {
        Frame {
            runs: self.entries.iter().map(|entry| entry.run.clone()).collect(),
        }
    }
}

fn to_run(translated: &Translated, symbols: &Symbols) -> Option<Run> {
    if symbols.hides(&translated.keysym) {
        return None;
    }

    let printable = translated.text.as_deref().filter(is_printable);

    let run = match (translated.modifiers.is_chord(), printable) {
        (true, key) => Run {
            text: chord_label(translated, key),
            kind: RunKind::Chord,
        },
        // A space types something, but nothing you can see, so a glyph for it
        // belongs with the text rather than beside it.
        (false, Some(text)) => Run {
            text: symbols.glyph(&translated.keysym).unwrap_or(text).to_owned(),
            kind: RunKind::Text,
        },
        (false, None) => Run {
            text: symbols
                .glyph(&translated.keysym)
                .unwrap_or(&translated.keysym)
                .to_owned(),
            kind: RunKind::Special,
        },
    };

    Some(run)
}

fn chord_label(translated: &Translated, printable: Option<&str>) -> String {
    let mut parts = translated.modifiers.names();
    let key = printable.unwrap_or(&translated.keysym);
    parts.push(key);
    parts.join("+")
}

fn is_printable(text: &&str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Modifiers;
    use crate::symbols::SPACE;

    fn typed(text: &str, modifiers: Modifiers) -> Translated {
        Translated {
            text: Some(text.to_owned()),
            keysym: text.to_owned(),
            modifiers,
        }
    }

    fn model(timeout: Duration) -> Model {
        Model::new(timeout, Symbols::names())
    }

    fn at(millis: u64) -> Timestamp {
        Timestamp::since_start(Duration::from_millis(millis))
    }

    #[test]
    fn plain_text_is_shown_as_typed() {
        let mut model = model(Duration::from_secs(1));
        model.record(&typed("h", Modifiers::default()), at(0));
        model.record(&typed("i", Modifiers::default()), at(10));

        assert_eq!(model.frame().to_plain(), "hi");
    }

    #[test]
    fn shift_produces_text_rather_than_a_chord() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let mut model = model(Duration::from_secs(1));
        model.record(&typed("A", shift), at(0));

        assert_eq!(
            model.frame().runs.first().map(|run| run.kind),
            Some(RunKind::Text)
        );
    }

    #[test]
    fn a_chord_names_its_modifiers() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let mut model = model(Duration::from_secs(1));
        model.record(
            &Translated {
                text: None,
                keysym: "c".to_owned(),
                modifiers: ctrl,
            },
            at(0),
        );

        assert_eq!(model.frame().to_plain(), "Ctrl+c");
    }

    #[test]
    fn a_key_without_printable_output_falls_back_to_its_keysym() {
        let mut model = model(Duration::from_secs(1));
        model.record(
            &Translated {
                text: Some("\r".to_owned()),
                keysym: "Return".to_owned(),
                modifiers: Modifiers::default(),
            },
            at(0),
        );

        assert_eq!(model.frame().to_plain(), "Return");
    }

    #[test]
    fn the_line_survives_until_the_last_key_goes_stale() {
        let mut model = model(Duration::from_millis(500));
        model.record(&typed("a", Modifiers::default()), at(0));
        model.record(&typed("b", Modifiers::default()), at(400));

        model.expire(at(600));
        assert_eq!(model.frame().to_plain(), "ab");

        model.expire(at(900));
        assert!(model.frame().is_empty());
    }

    #[test]
    fn glyphs_replace_the_names_of_keys_that_have_them() {
        let mut model = Model::new(Duration::from_secs(1), Symbols::glyphs());
        model.record(
            &Translated {
                text: Some("\r".to_owned()),
                keysym: "Return".to_owned(),
                modifiers: Modifiers::default(),
            },
            at(0),
        );

        assert_eq!(model.frame().to_plain(), "⏎");
    }

    #[test]
    fn a_space_becomes_visible_without_breaking_the_word_it_sits_in() {
        let mut model = Model::new(Duration::from_secs(1), Symbols::glyphs());
        for (text, keysym) in [("h", "h"), (" ", SPACE), ("i", "i")] {
            model.record(
                &Translated {
                    text: Some(text.to_owned()),
                    keysym: keysym.to_owned(),
                    modifiers: Modifiers::default(),
                },
                at(0),
            );
        }

        assert_eq!(model.frame().to_plain(), "h␣i");
    }

    #[test]
    fn a_key_hidden_by_an_override_is_not_recorded() {
        let hidden = std::collections::HashMap::from([(SPACE.to_owned(), String::new())]);
        let mut model = Model::new(
            Duration::from_secs(1),
            Symbols::glyphs().overridden(&hidden),
        );
        model.record(
            &Translated {
                text: Some(" ".to_owned()),
                keysym: SPACE.to_owned(),
                modifiers: Modifiers::default(),
            },
            at(0),
        );

        assert!(model.frame().is_empty());
    }

    #[test]
    fn the_line_stops_growing_at_the_cap() {
        let mut model = model(Duration::from_secs(60));
        for _ in 0..MAX_RUNS + 10 {
            model.record(&typed("a", Modifiers::default()), at(0));
        }

        assert_eq!(model.frame().runs.len(), MAX_RUNS);
    }
}
