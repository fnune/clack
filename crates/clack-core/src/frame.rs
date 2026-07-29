#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Text,
    Chord,
    Special,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub kind: RunKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub runs: Vec<Run>,
}

impl Frame {
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    pub fn to_plain(&self) -> String {
        let mut out = String::new();
        let mut previous: Option<RunKind> = None;

        for run in &self.runs {
            let needs_gap = match previous {
                None => false,
                Some(RunKind::Text) => run.kind != RunKind::Text,
                Some(_) => true,
            };
            if needs_gap {
                out.push(' ');
            }
            out.push_str(&run.text);
            previous = Some(run.kind);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, kind: RunKind) -> Run {
        Run {
            text: text.to_owned(),
            kind,
        }
    }

    #[test]
    fn adjacent_text_runs_join_without_a_gap() {
        let frame = Frame {
            runs: vec![
                run("h", RunKind::Text),
                run("i", RunKind::Text),
                run("Return", RunKind::Special),
                run("Ctrl+C", RunKind::Chord),
            ],
        };

        assert_eq!(frame.to_plain(), "hi Return Ctrl+C");
    }
}
