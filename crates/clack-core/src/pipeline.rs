use core::fmt;
use core::time::Duration;

use crate::clock::Clock;
use crate::model::Model;
use crate::ports::{KeySource, Surface, Translator};

#[derive(Debug)]
pub struct Pipeline<S, T, U, C> {
    source: S,
    translator: T,
    surface: U,
    clock: C,
    model: Model,
}

impl<S, T, U, C> Pipeline<S, T, U, C>
where
    S: KeySource,
    T: Translator,
    U: Surface,
    C: Clock,
{
    pub fn new(source: S, translator: T, surface: U, clock: C, timeout: Duration) -> Self {
        Self {
            source,
            translator,
            surface,
            clock,
            model: Model::new(timeout),
        }
    }

    pub fn step(&mut self) -> Result<(), PipelineError<S::Error, U::Error>> {
        let events = self.source.next_batch().map_err(PipelineError::Source)?;
        let now = self.clock.now();

        for event in events {
            if let Some(translated) = self.translator.translate(event) {
                self.model.record(&translated, now);
            }
        }

        self.model.expire(now);
        self.surface
            .present(&self.model.frame())
            .map_err(PipelineError::Surface)
    }
}

#[derive(Debug)]
pub enum PipelineError<S, U> {
    Source(S),
    Surface(U),
}

impl<S: fmt::Display, U: fmt::Display> fmt::Display for PipelineError<S, U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "reading key events failed: {error}"),
            Self::Surface(error) => write!(f, "presenting the overlay failed: {error}"),
        }
    }
}

impl<S, U> core::error::Error for PipelineError<S, U>
where
    S: core::error::Error + 'static,
    U: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Surface(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Timestamp;
    use crate::frame::Frame;
    use crate::key::{EvdevCode, KeyEvent, KeyState, Modifiers, Translated};
    use core::convert::Infallible;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct ScriptedSource(Vec<Vec<KeyEvent>>);

    impl KeySource for ScriptedSource {
        type Error = Infallible;

        fn next_batch(&mut self) -> Result<Vec<KeyEvent>, Self::Error> {
            Ok(if self.0.is_empty() {
                Vec::new()
            } else {
                self.0.remove(0)
            })
        }
    }

    struct LetterTranslator;

    impl Translator for LetterTranslator {
        fn translate(&mut self, event: KeyEvent) -> Option<Translated> {
            if event.state == KeyState::Released {
                return None;
            }
            let letter =
                char::from(b'a'.saturating_add(u8::try_from(event.code.raw()).unwrap_or(0)));
            Some(Translated {
                text: Some(letter.to_string()),
                keysym: letter.to_string(),
                modifiers: Modifiers::default(),
            })
        }
    }

    #[derive(Default)]
    struct RecordingSurface(Rc<RefCell<Vec<Frame>>>);

    impl Surface for RecordingSurface {
        type Error = Infallible;

        fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
            self.0.borrow_mut().push(frame.clone());
            Ok(())
        }
    }

    struct ManualClock(Rc<RefCell<Duration>>);

    impl Clock for ManualClock {
        fn now(&self) -> Timestamp {
            Timestamp::since_start(*self.0.borrow())
        }
    }

    fn press(code: u16) -> KeyEvent {
        KeyEvent {
            code: EvdevCode::new(code),
            state: KeyState::Pressed,
            at: Timestamp::ZERO,
        }
    }

    #[test]
    fn a_batch_of_presses_becomes_one_frame() {
        let frames = Rc::new(RefCell::new(Vec::new()));
        let now = Rc::new(RefCell::new(Duration::ZERO));

        let mut pipeline = Pipeline::new(
            ScriptedSource(vec![vec![press(0), press(1)]]),
            LetterTranslator,
            RecordingSurface(Rc::clone(&frames)),
            ManualClock(Rc::clone(&now)),
            Duration::from_secs(1),
        );

        pipeline.step().unwrap();

        assert_eq!(
            frames.borrow().last().map(Frame::to_plain),
            Some("ab".to_owned())
        );
    }

    #[test]
    fn the_frame_empties_once_the_timeout_passes() {
        let frames = Rc::new(RefCell::new(Vec::new()));
        let now = Rc::new(RefCell::new(Duration::ZERO));

        let mut pipeline = Pipeline::new(
            ScriptedSource(vec![vec![press(0)]]),
            LetterTranslator,
            RecordingSurface(Rc::clone(&frames)),
            ManualClock(Rc::clone(&now)),
            Duration::from_millis(500),
        );

        pipeline.step().unwrap();
        *now.borrow_mut() = Duration::from_millis(600);
        pipeline.step().unwrap();

        assert_eq!(frames.borrow().last().map(Frame::is_empty), Some(true));
    }
}
