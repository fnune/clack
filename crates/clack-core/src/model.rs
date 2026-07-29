use core::time::Duration;
use std::collections::VecDeque;

use crate::clock::Timestamp;
use crate::frame::{Frame, Run, RunKind};
use crate::key::Translated;

#[derive(Clone, Debug)]
struct Entry {
    run: Run,
    at: Timestamp,
}

const MAX_RUNS: usize = 48;

#[derive(Clone, Debug)]
pub struct Model {
    timeout: Duration,
    entries: VecDeque<Entry>,
}

impl Model {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            entries: VecDeque::new(),
        }
    }

    pub fn record(&mut self, translated: &Translated, now: Timestamp) {
        self.entries.push_back(Entry {
            run: to_run(translated),
            at: now,
        });

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

fn to_run(translated: &Translated) -> Run {
    let printable = translated.text.as_deref().filter(is_printable);

    match (translated.modifiers.is_chord(), printable) {
        (true, key) => Run {
            text: chord_label(translated, key),
            kind: RunKind::Chord,
        },
        (false, Some(text)) => Run {
            text: text.to_owned(),
            kind: RunKind::Text,
        },
        (false, None) => Run {
            text: translated.keysym.clone(),
            kind: RunKind::Special,
        },
    }
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

    fn typed(text: &str, modifiers: Modifiers) -> Translated {
        Translated {
            text: Some(text.to_owned()),
            keysym: text.to_owned(),
            modifiers,
        }
    }

    fn at(millis: u64) -> Timestamp {
        Timestamp::since_start(Duration::from_millis(millis))
    }

    #[test]
    fn plain_text_is_shown_as_typed() {
        let mut model = Model::new(Duration::from_secs(1));
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
        let mut model = Model::new(Duration::from_secs(1));
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
        let mut model = Model::new(Duration::from_secs(1));
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
        let mut model = Model::new(Duration::from_secs(1));
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
        let mut model = Model::new(Duration::from_millis(500));
        model.record(&typed("a", Modifiers::default()), at(0));
        model.record(&typed("b", Modifiers::default()), at(400));

        model.expire(at(600));
        assert_eq!(model.frame().to_plain(), "ab");

        model.expire(at(900));
        assert!(model.frame().is_empty());
    }

    #[test]
    fn the_line_stops_growing_at_the_cap() {
        let mut model = Model::new(Duration::from_secs(60));
        for _ in 0..MAX_RUNS + 10 {
            model.record(&typed("a", Modifiers::default()), at(0));
        }

        assert_eq!(model.frame().runs.len(), MAX_RUNS);
    }
}
