//! Clack as a library: read the keyboard, name the keys, show them somewhere.
//!
//! The smallest useful program draws the overlay:
//!
//! ```no_run
//! let mut session = clack::Session::overlay(&clack::Config::default())?;
//! session.run()?;
//! # Ok::<(), clack::SessionError>(())
//! ```
//!
//! Anything implementing [`Surface`] can take the overlay's place, which is how
//! a terminal, a tray applet, or a browser would each show the same keystrokes.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod clock;
pub mod config;
pub mod diagnostics;
pub mod input;
pub mod keymap;
pub mod render;
pub mod session;
pub mod text;
pub mod wayland;

pub use clack_core::{Frame, Run, RunKind, Surface};
pub use config::{Anchor, Config, Style};
pub use diagnostics::{CompositorReport, KeymapSource, Report};
pub use session::{Handle, Session, SessionError, State};
pub use text::TextSurface;
