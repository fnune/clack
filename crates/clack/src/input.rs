use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::thread;
use std::time::Duration;
use std::{fs, io};

use clack_core::{Clock, EvdevCode, KeyEvent, KeySource, KeyState};
use evdev::{Device, EventType, KeyCode};

const DEVICE_DIR: &str = "/dev/input";

const EXPIRY_TICK: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct DeviceReport {
    pub path: PathBuf,
    pub name: Option<String>,
    pub access: Access,
}

#[derive(Debug)]
pub enum Access {
    Keyboard,
    NotAKeyboard,
    Denied,
    Failed(io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("no readable keyboard found under {DEVICE_DIR}")]
    NoKeyboards,
    #[error("every keyboard reader stopped")]
    Disconnected,
    #[error("listing {DEVICE_DIR} failed: {0}")]
    Listing(#[from] io::Error),
}

pub fn survey() -> Result<Vec<DeviceReport>, SourceError> {
    let mut reports = Vec::new();

    for path in event_device_paths()? {
        let report = match Device::open(&path) {
            Ok(device) => DeviceReport {
                name: device.name().map(str::to_owned),
                access: if is_keyboard(&device) {
                    Access::Keyboard
                } else {
                    Access::NotAKeyboard
                },
                path,
            },
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => DeviceReport {
                path,
                name: None,
                access: Access::Denied,
            },
            Err(error) => DeviceReport {
                path,
                name: None,
                access: Access::Failed(error),
            },
        };
        reports.push(report);
    }

    reports.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(reports)
}

fn event_device_paths() -> Result<Vec<PathBuf>, SourceError> {
    let mut paths = Vec::new();

    for entry in fs::read_dir(Path::new(DEVICE_DIR))? {
        let path = entry?.path();
        let is_event_device = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("event"));
        if is_event_device {
            paths.push(path);
        }
    }

    Ok(paths)
}

fn is_keyboard(device: &Device) -> bool {
    // Power buttons and lid switches report key codes too, so EV_KEY alone would match them.
    const TYPING_KEYS: [KeyCode; 3] = [KeyCode::KEY_A, KeyCode::KEY_Z, KeyCode::KEY_SPACE];

    device
        .supported_keys()
        .is_some_and(|keys| TYPING_KEYS.into_iter().all(|key| keys.contains(key)))
}

#[derive(Debug)]
pub struct EvdevSource {
    events: Receiver<KeyEvent>,
}

impl EvdevSource {
    pub fn open<C: Clock + Clone + Send + 'static>(clock: C) -> Result<Self, SourceError> {
        let keyboards: Vec<PathBuf> = survey()?
            .into_iter()
            .filter(|report| matches!(report.access, Access::Keyboard))
            .map(|report| report.path)
            .collect();

        if keyboards.is_empty() {
            return Err(SourceError::NoKeyboards);
        }

        let (sender, events) = channel();
        for path in keyboards {
            if let Ok(device) = Device::open(&path) {
                let sender = sender.clone();
                let clock = clock.clone();
                thread::spawn(move || pump(device, &sender, &clock));
            }
        }

        Ok(Self { events })
    }
}

fn pump<C: Clock>(mut device: Device, sender: &Sender<KeyEvent>, clock: &C) {
    loop {
        let Ok(batch) = device.fetch_events() else {
            return;
        };

        for event in batch {
            if event.event_type() != EventType::KEY {
                continue;
            }
            let Some(state) = key_state(event.value()) else {
                continue;
            };
            let key = KeyEvent {
                code: EvdevCode::new(event.code()),
                state,
                at: clock.now(),
            };
            if sender.send(key).is_err() {
                return;
            }
        }
    }
}

const fn key_state(value: i32) -> Option<KeyState> {
    match value {
        0 => Some(KeyState::Released),
        1 => Some(KeyState::Pressed),
        2 => Some(KeyState::Repeated),
        _ => None,
    }
}

impl KeySource for EvdevSource {
    type Error = SourceError;

    fn next_batch(&mut self) -> Result<Vec<KeyEvent>, Self::Error> {
        let mut batch = match self.events.recv_timeout(EXPIRY_TICK) {
            Ok(event) => vec![event],
            Err(RecvTimeoutError::Timeout) => return Ok(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => return Err(SourceError::Disconnected),
        };

        loop {
            match self.events.try_recv() {
                Ok(event) => batch.push(event),
                Err(TryRecvError::Empty) => return Ok(batch),
                Err(TryRecvError::Disconnected) => return Ok(batch),
            }
        }
    }
}
