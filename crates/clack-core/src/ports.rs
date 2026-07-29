//! The abstractions the domain consumes. The input, keymap, and render layers
//! implement them; the domain never names an implementation.

use crate::frame::Frame;
use crate::key::{KeyEvent, Translated};

pub trait KeySource {
    type Error;

    /// Waits for one event, then drains. An empty batch means open but idle.
    fn next_batch(&mut self) -> Result<Vec<KeyEvent>, Self::Error>;
}

pub trait Translator {
    /// `None` for events with nothing to show, such as key releases.
    fn translate(&mut self, event: KeyEvent) -> Option<Translated>;
}

pub trait Surface {
    type Error;

    fn present(&mut self, frame: &Frame) -> Result<(), Self::Error>;
}
