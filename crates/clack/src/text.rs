use std::io::{self, Write};

use clack_core::{Frame, Surface};

/// Writes each new line of keystrokes to a writer. Useful on its own, and as
/// the smallest possible example of implementing [`Surface`].
#[derive(Debug)]
pub struct TextSurface<W> {
    writer: W,
    shown: String,
}

impl<W: Write> TextSurface<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            shown: String::new(),
        }
    }
}

impl<W: Write> Surface for TextSurface<W> {
    type Error = io::Error;

    fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
        let line = frame.to_plain();
        if line == self.shown {
            return Ok(());
        }

        self.shown = line;
        if !self.shown.is_empty() {
            writeln!(self.writer, "{}", self.shown)?;
            self.writer.flush()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clack_core::{Run, RunKind};

    fn frame(text: &str) -> Frame {
        Frame {
            runs: vec![Run {
                text: text.to_owned(),
                kind: RunKind::Text,
            }],
        }
    }

    #[test]
    fn only_changed_lines_are_written() {
        let mut surface = TextSurface::new(Vec::new());

        surface.present(&frame("a")).unwrap();
        surface.present(&frame("a")).unwrap();
        surface.present(&frame("b")).unwrap();
        surface.present(&Frame::default()).unwrap();

        assert_eq!(String::from_utf8(surface.writer).unwrap(), "a\nb\n");
    }
}
