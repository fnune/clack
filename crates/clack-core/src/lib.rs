//! Domain layer for Clack. Links no system libraries and opens no devices, so
//! it builds and tests without a compositor, input devices, or privileges.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod clock;
mod frame;
mod key;
mod model;
mod pipeline;
mod ports;

pub use clock::{Clock, Timestamp};
pub use frame::{Frame, Run, RunKind};
pub use key::{EvdevCode, KeyEvent, KeyState, Modifiers, Translated, XkbKeycode};
pub use model::Model;
pub use pipeline::{Pipeline, PipelineError};
pub use ports::{KeySource, Surface, Translator};
