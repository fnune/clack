use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use clack::{Config, Session, State};

/// How long to wait for a session to open its devices and its surface before
/// treating the attempt as failed.
const STARTUP: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("{0}")]
    Start(String),
    #[error("showing keystrokes did not start within {}s", STARTUP.as_secs())]
    TooSlow,
}

#[derive(Debug, Default)]
pub struct Supervisor {
    running: Option<Running>,
}

#[derive(Debug)]
struct Running {
    handle: clack::Handle,
    thread: JoinHandle<()>,
    failure: Arc<Mutex<Option<String>>>,
}

impl Supervisor {
    pub fn start(&mut self, config: Config) -> Result<(), SupervisorError> {
        self.stop();

        let failure = Arc::new(Mutex::new(None));
        let reported = Arc::clone(&failure);
        let (opened, ready) = channel();

        let thread = thread::spawn(move || match Session::overlay(&config) {
            Err(error) => {
                let _ = opened.send(Err(error.to_string()));
            }
            Ok(mut session) => {
                let _ = opened.send(Ok(session.handle()));
                if let Err(error) = session.run() {
                    record(&reported, error.to_string());
                }
            }
        });

        match ready.recv_timeout(STARTUP) {
            Ok(Ok(handle)) => {
                self.running = Some(Running {
                    handle,
                    thread,
                    failure,
                });
                Ok(())
            }
            Ok(Err(error)) => Err(SupervisorError::Start(error)),
            Err(RecvTimeoutError::Timeout) => Err(SupervisorError::TooSlow),
            Err(RecvTimeoutError::Disconnected) => Err(SupervisorError::Start(
                "showing keystrokes stopped before it started".to_owned(),
            )),
        }
    }

    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };

        running.handle.stop();
        let _ = running.thread.join();
    }

    pub fn state(&self) -> Option<State> {
        self.running.as_ref().map(|running| running.handle.state())
    }

    /// Why the session stopped on its own, if it did.
    pub fn failure(&self) -> Option<String> {
        self.running
            .as_ref()
            .and_then(|running| running.failure.lock().ok()?.clone())
    }
}

fn record(failure: &Mutex<Option<String>>, message: String) {
    if let Ok(mut slot) = failure.lock() {
        *slot = Some(message);
    }
}
