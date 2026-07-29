use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use clack_core::{Pipeline, PipelineError, Surface};

use crate::clock::MonotonicClock;
use crate::config::Config;
use crate::input::{EvdevSource, SourceError};
use crate::keymap::{KeymapError, XkbTranslator};
use crate::render::{LayerOverlay, RenderError};
use crate::wayland::{self, WaylandError};

const RUNNING: u8 = 0;
const PAUSED: u8 = 1;
const STOPPED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Stopped,
}

/// Controls a running session from anywhere, including another thread.
#[derive(Clone, Debug, Default)]
pub struct Handle(Arc<AtomicU8>);

impl Handle {
    pub fn state(&self) -> State {
        match self.0.load(Ordering::Relaxed) {
            PAUSED => State::Paused,
            STOPPED => State::Stopped,
            _ => State::Running,
        }
    }

    /// Keeps reading keys but shows nothing, so a pause leaves no trace of what
    /// was typed while it was on.
    pub fn pause(&self) {
        self.set(PAUSED);
    }

    pub fn resume(&self) {
        self.set(RUNNING);
    }

    pub fn stop(&self) {
        self.set(STOPPED);
    }

    fn set(&self, state: u8) {
        self.0.store(state, Ordering::Relaxed);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Devices(#[from] SourceError),
    #[error(transparent)]
    Compositor(#[from] WaylandError),
    #[error(transparent)]
    Keymap(#[from] KeymapError),
    #[error(transparent)]
    Overlay(#[from] RenderError),
    #[error("the compositor did not send a keymap, so keys cannot be named")]
    NoKeymap,
    #[error("showing the keystrokes failed: {0}")]
    Surface(#[source] Box<dyn Error + Send + Sync>),
}

/// A running Clack: devices, keymap, model, and somewhere to show the result.
/// Callers choose the surface, so the overlay, a terminal, or a future UI are
/// all the same thing to this type.
#[derive(Debug)]
pub struct Session<S> {
    pipeline: Pipeline<EvdevSource, XkbTranslator, S, MonotonicClock>,
    handle: Handle,
}

impl Session<LayerOverlay> {
    pub fn overlay(config: &Config) -> Result<Self, SessionError> {
        let overlay = LayerOverlay::open(config.style.clone())?;
        Self::with_surface(overlay, config)
    }
}

impl<S: Surface> Session<S> {
    pub fn with_surface(surface: S, config: &Config) -> Result<Self, SessionError> {
        let clock = MonotonicClock::started();

        Ok(Self {
            pipeline: Pipeline::new(
                EvdevSource::open(clock)?,
                translator()?,
                surface,
                clock,
                config.timeout,
            ),
            handle: Handle::default(),
        })
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn surface(&self) -> &S {
        self.pipeline.surface()
    }
}

impl<S: Surface> Session<S>
where
    S::Error: Error + Send + Sync + 'static,
{
    pub fn step(&mut self) -> Result<State, SessionError> {
        if self.pipeline.surface().is_closed() {
            self.handle.stop();
        }

        let state = self.handle.state();
        match state {
            State::Running => self.pipeline.step().map_err(as_session_error)?,
            State::Paused => self.pipeline.discard().map_err(as_session_error)?,
            State::Stopped => {}
        }

        Ok(state)
    }

    /// Runs until the surface closes or the handle says to stop.
    pub fn run(&mut self) -> Result<(), SessionError> {
        while self.step()? != State::Stopped {}
        Ok(())
    }
}

fn translator() -> Result<XkbTranslator, SessionError> {
    let keymap = wayland::probe()?.keymap.ok_or(SessionError::NoKeymap)?;

    Ok(XkbTranslator::from_text(&keymap)?)
}

fn as_session_error<E: Error + Send + Sync + 'static>(
    error: PipelineError<SourceError, E>,
) -> SessionError {
    match error {
        PipelineError::Source(error) => SessionError::Devices(error),
        PipelineError::Surface(error) => SessionError::Surface(Box::new(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_starts_running_and_reaches_every_state() {
        let handle = Handle::default();
        assert_eq!(handle.state(), State::Running);

        handle.pause();
        assert_eq!(handle.state(), State::Paused);

        handle.resume();
        assert_eq!(handle.state(), State::Running);

        handle.stop();
        assert_eq!(handle.state(), State::Stopped);
    }

    #[test]
    fn a_cloned_handle_controls_the_same_session() {
        let handle = Handle::default();
        let remote = handle.clone();

        remote.stop();

        assert_eq!(handle.state(), State::Stopped);
    }
}
