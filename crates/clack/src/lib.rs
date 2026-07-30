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
//! a terminal, a tray applet, or a browser would each show the same keystrokes:
//!
//! ```no_run
//! use clack::{Config, Frame, LiveSession, Surface};
//!
//! struct Caption(String);
//!
//! impl Surface for Caption {
//!     type Error = std::convert::Infallible;
//!
//!     fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
//!         self.0 = frame.runs.iter().map(|run| run.text.as_str()).collect();
//!         Ok(())
//!     }
//! }
//!
//! let config = Config::default();
//! let mut session = LiveSession::with_surface(Caption(String::new()), &config)?;
//! let handle = session.handle();
//!
//! std::thread::spawn(move || {
//!     handle.pause();
//!     handle.resume();
//!     handle.stop();
//! });
//!
//! session.run()?;
//! # Ok::<(), clack::SessionError>(())
//! ```
//!
//! [`Handle`] is the only thing that crosses threads. Reading, naming and
//! drawing all happen on the thread that called `run`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod access;
pub mod clock;
pub mod config;
pub mod diagnostics;
pub mod input;
pub mod keymap;
pub mod render;
pub mod session;
pub mod settings;
pub mod text;
pub mod wayland;

pub use access::{AccessError, RuleStatus};
pub use clack_core::Model;
pub use clack_core::{
    Frame, Modifiers, Run, RunKind, SPACE, Surface, Symbols, Timestamp, Translated,
};
pub use config::{Anchor, Config, Style};
pub use diagnostics::{CompositorReport, KeymapSource, Report};
pub use session::{Handle, LiveSession, Session, SessionError, State};
pub use settings::{Desktop, Overlay, Settings, SettingsError};
pub use text::TextSurface;
